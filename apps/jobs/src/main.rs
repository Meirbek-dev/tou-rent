//! Фоновый воркер (арх. § 2): двигатель обязательств-сроков (FR-1702).
//!
//! Раз в интервал проверяет `core.obligations`: просроченные переводит в
//! `overdue` и уведомляет носителей роли-исполнителя (эскалация, п. 54, 57,
//! 73, 75). Перевод, выборка получателей и запись уведомлений идут одной
//! транзакцией слоя данных, а само уведомление однократно (`escalated_at`) -
//! воркер можно запускать чаще, перезапускать и держать в нескольких
//! экземплярах.
//!
//! Третьим проходом закрывает торги, у которых истек server-authoritative
//! таймер (INV-066, п. 66, 68): ставку после `ends_at` БД и так не примет,
//! но без завершения комната остается `running` - победитель не определен,
//! а срок протокола итогов (п. 73) не открыт.
//!
//! Отдельным, редким расписанием сверяет hash-цепочку аудита (INV-A01):
//! доказательность системы держится на ней, а разрыв виден только проверкой.
//! Итог каждой сверки ложится в `audit.chain_checks` - иначе он остался бы
//! строкой в журнале контейнера, которую некому прочитать (арх. v3 § 8).
//!
//! Вторым проходом снимает с публичного доступа протоколы, у которых истек
//! шестимесячный срок (INV-076, п. 76): сам протокол остается в досье и в
//! кабинете участника - снимается только публичность. Тем же проходом
//! снимаются публикации особого порядка (FR-1403, п. 90, 92, 97): результаты,
//! обоснования ставок и акты приемки живут на портале те же шесть месяцев.

use std::time::Duration;

use anyhow::Context;

/// Как часто проверять сроки. Сроки Правил измеряются днями, поэтому минута
/// - с большим запасом; переопределяется для демо и тестов.
const DEFAULT_INTERVAL: Duration = Duration::from_secs(60);

/// Как часто сверять цепочку аудита (INV-A01). Полный проход по журналу
/// стоит дороже прочих, а разрыв - событие не минутного масштаба: раз в час
/// достаточно, чтобы заметить его в тот же рабочий день.
const DEFAULT_AUDIT_INTERVAL: Duration = Duration::from_secs(3_600);

/// Сколько торгов закрывать за проход. Одновременных аукционов единицы
/// (арх. § 2), потолок - страховка от долгой транзакции.
const AUCTION_BATCH: i64 = 32;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let database_url =
        std::env::var("DATABASE_URL").context("переменная DATABASE_URL не задана")?;
    let interval = interval_from_env()?;

    let db = tou_db::connect(&database_url)
        .await
        .context("подключение PostgreSQL")?;

    let audit_interval = audit_interval_from_env()?;
    tracing::info!(
        ?interval,
        ?audit_interval,
        "jobs: двигатель обязательств запущен"
    );
    let mut ticker = tokio::time::interval(interval);
    let mut audit_ticker = tokio::time::interval(audit_interval);
    loop {
        // Сигнал ловится между проходами, а не внутри: проход короткий,
        // а прерванный посередине оставил бы часть сроков разобранной,
        // часть - нет. Незавершенное подберет следующий запуск
        let tick = tokio::select! {
            _ = ticker.tick() => Tick::Work,
            _ = audit_ticker.tick() => Tick::Audit,
            () = shutdown_signal() => Tick::Stop,
        };

        match tick {
            Tick::Stop => {
                tracing::info!("jobs: остановка по сигналу");
                return Ok(());
            }
            Tick::Audit => {
                if let Err(error) = verify_audit_chain(&db).await {
                    tracing::error!(%error, "jobs: проверка цепочки аудита не удалась");
                }
                continue;
            }
            Tick::Work => {}
        }

        if let Err(error) = escalate_overdue(&db).await {
            // Ошибка одного прохода не должна ронять воркер: следующий тик
            // повторит работу (обязательства остаются в БД)
            tracing::error!(%error, "jobs: проверка сроков не удалась");
        }
        if let Err(error) = unpublish_expired(&db).await {
            tracing::error!(%error, "jobs: снятие публикаций не удалось");
        }
        if let Err(error) = unpublish_expired_records(&db).await {
            tracing::error!(%error, "jobs: снятие публикаций особого порядка не удалось");
        }
        if let Err(error) = finish_expired_auctions(&db).await {
            tracing::error!(%error, "jobs: завершение истекших торгов не удалось");
        }
    }
}

/// Что разбудило воркер.
enum Tick {
    /// Обычный проход: сроки, публикации, истекшие торги
    Work,
    /// Редкая сверка цепочки аудита (INV-A01)
    Audit,
    Stop,
}

/// Сигнал остановки: SIGTERM от оркестратора, Ctrl+C у разработчика.
///
/// Копия такой же функции в `apps/api`: общий крейт под двадцать строк
/// подписки на сигналы пришлось бы вешать на слой, который к сигналам
/// отношения не имеет (`domain` без tokio, `http` - не про воркер).
async fn shutdown_signal() {
    let interrupt = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "подписка на Ctrl+C не удалась");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "подписка на SIGTERM не удалась");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = interrupt => {}
        () = terminate => {}
    }
}

/// `JOBS_INTERVAL` в секундах (для демо и тестов), по умолчанию - минута.
fn interval_from_env() -> anyhow::Result<Duration> {
    match std::env::var("JOBS_INTERVAL") {
        Ok(raw) => {
            let seconds: u64 = raw
                .trim()
                .parse()
                .with_context(|| format!("JOBS_INTERVAL «{raw}»: ожидались секунды"))?;
            anyhow::ensure!(seconds > 0, "JOBS_INTERVAL должен быть положительным");
            Ok(Duration::from_secs(seconds))
        }
        Err(_) => Ok(DEFAULT_INTERVAL),
    }
}

/// `AUDIT_VERIFY_INTERVAL` в секундах, по умолчанию - час.
fn audit_interval_from_env() -> anyhow::Result<Duration> {
    match std::env::var("AUDIT_VERIFY_INTERVAL") {
        Ok(raw) => {
            let seconds: u64 = raw
                .trim()
                .parse()
                .with_context(|| format!("AUDIT_VERIFY_INTERVAL «{raw}»: ожидались секунды"))?;
            anyhow::ensure!(
                seconds > 0,
                "AUDIT_VERIFY_INTERVAL должен быть положительным"
            );
            Ok(Duration::from_secs(seconds))
        }
        Err(_) => Ok(DEFAULT_AUDIT_INTERVAL),
    }
}

/// Один проход: торги с истекшим таймером закрываются (INV-066, FR-606).
///
/// Итог считает тот же код, что и при завершении председателем, поэтому
/// победитель и второе место определяются одинаково независимо от того,
/// нажал кто-нибудь кнопку или нет.
async fn finish_expired_auctions(db: &tou_db::Db) -> anyhow::Result<usize> {
    let finished = tou_db::auctions::finish_expired(db, AUCTION_BATCH)
        .await
        .context("завершение торгов с истекшим таймером")?;
    if finished.is_empty() {
        return Ok(0);
    }

    for auction in &finished {
        tracing::info!(
            auction_id = %auction.id,
            lot_id = %auction.lot_id,
            winner = ?auction.winner_application_id,
            "jobs: торги закрыты по истечении времени (п. 66, 68)"
        );
    }
    Ok(finished.len())
}

/// Один проход: сверка hash-цепочки аудита (INV-A01) с записью результата.
///
/// Разрыв - это не рядовая ошибка прохода, а признак того, что историю
/// переписали в обход append-only. Логом дело не ограничивается: наружу
/// логи не уходят, а журнал контейнера ротируется, поэтому результат каждой
/// сверки - и целой цепочки, и разорванной - остается в `audit.chain_checks`,
/// откуда его читает кабинет админа. Молча пропущенная сверка (воркер стоял,
/// БД была недоступна) видна там же по возрасту последней записи.
async fn verify_audit_chain(db: &tou_db::Db) -> anyhow::Result<()> {
    let check = tou_db::audit::run_chain_check(db)
        .await
        .context("сверка цепочки аудита")?;

    if check.intact {
        tracing::info!(
            entries = check.entries,
            "jobs: цепочка аудита цела (INV-A01)"
        );
    } else {
        tracing::error!(
            entries = check.entries,
            broken_at = ?check.broken_at,
            "jobs: ЦЕПОЧКА АУДИТА РАЗОРВАНА (INV-A01) - журнал изменен в обход append-only"
        );
    }
    Ok(())
}

/// Один проход: просроченные сроки → уведомления исполнителям (FR-1702).
///
/// Отметка о просрочке и уведомления пишутся одной транзакцией слоя данных.
/// Разложить их обратно на два шага здесь - значит вернуть потерю
/// уведомлений при первом же сбое между ними: следующий проход такой срок
/// уже не увидит.
async fn escalate_overdue(db: &tou_db::Db) -> anyhow::Result<usize> {
    let escalated = tou_db::obligations::take_overdue(db)
        .await
        .context("эскалация просроченных обязательств")?;
    if escalated.is_empty() {
        return Ok(0);
    }

    tracing::info!(
        count = escalated.len(),
        "jobs: эскалация просроченных сроков"
    );
    Ok(escalated.len())
}

/// Один проход: протоколы с истекшим шестимесячным сроком публичного
/// доступа снимаются с публикации (INV-076, п. 76). Материал остается
/// в досье тендера - запись туда делает триггер БД.
async fn unpublish_expired(db: &tou_db::Db) -> anyhow::Result<usize> {
    let taken = tou_db::publications::take_expired(db)
        .await
        .context("снятие протоколов с публикации")?;
    if taken.is_empty() {
        return Ok(0);
    }

    for protocol in &taken {
        tracing::info!(
            protocol_id = %protocol.id,
            tender_id = %protocol.tender_id,
            kind = %protocol.kind,
            "jobs: публичный доступ к протоколу снят (п. 76)"
        );
    }
    Ok(taken.len())
}

/// Один проход: публикации особого порядка с истекшим шестимесячным сроком
/// снимаются с портала (FR-1403, INV-076). Материал остается в досье решения
/// (FR-1206) - запись туда делает триггер БД.
async fn unpublish_expired_records(db: &tou_db::Db) -> anyhow::Result<usize> {
    let taken = tou_db::public_records::take_expired(db)
        .await
        .context("снятие публикаций особого порядка")?;
    if taken.is_empty() {
        return Ok(0);
    }

    for record in &taken {
        tracing::info!(
            record_id = %record.id,
            kind = %record.kind.as_str(),
            "jobs: публичный доступ к материалу особого порядка снят (п. 76, 97)"
        );
    }
    Ok(taken.len())
}
