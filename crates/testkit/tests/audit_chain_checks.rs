//! Журнал сверок hash-цепочки аудита (W-12, INV-A01, FR-1601).
//!
//! Сверка цепочки идет фоновым проходом раз в час, и до сих пор ее итог
//! существовал ровно в одном виде - строкой в stdout контейнера. Экспорта
//! логов и метрик у бэкенда нет (арх. v3 § 8, Q-018), журнал контейнера
//! ротируется: событие, ради обнаружения которого построена вся конструкция
//! append-only, обнаруживалось и тут же терялось. Проверяется, что теперь
//! у каждой сверки остается запись и что подделать ее обычным путем нельзя.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

async fn try_pool() -> Result<Option<tou_db::Db>, sqlx::Error> {
    // Пропуск без адреса допустим локально, но не в пайплайне (G2/G15):
    // молча пройденный интеграционный тест ничего не проверяет
    match tou_testkit::database_url().map_err(|e| sqlx::Error::Configuration(Box::new(e)))? {
        Some(url) => tou_db::connect(&url).await.map(Some),
        None => Ok(None),
    }
}

macro_rules! require_db {
    () => {
        match try_pool()
            .await
            .expect("TESTKIT_DATABASE_URL: подключение не удалось")
        {
            Some(db) => db,
            None => {
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - журнал сверок не проверялся");
                return;
            }
        }
    };
}

/// INV-A01: сверка оставляет след, и кабинет админа читает именно его.
#[tokio::test]
async fn inv_a01_chain_check_is_recorded_and_readable() {
    let db = require_db!();

    let before = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM audit.chain_checks"#)
        .fetch_one(&db)
        .await
        .expect("счетчик сверок");

    let check = tou_db::audit::run_chain_check(&db)
        .await
        .expect("сверка цепочки");
    assert!(check.intact, "INV-A01: hash-цепочка audit.log разорвана");
    assert!(
        check.entries > 0,
        "журнал аудита пуст - сверять нечего, проверьте стенд"
    );
    assert!(
        check.broken_at.is_none(),
        "у целой цепочки места расхождения быть не может"
    );

    let after = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM audit.chain_checks"#)
        .fetch_one(&db)
        .await
        .expect("счетчик сверок");
    assert_eq!(
        after,
        before + 1,
        "каждая сверка обязана оставить ровно одну запись"
    );

    // Тот же путь, которым состояние цепочки читает кабинет админа
    let state = tou_db::audit::chain_state(&db)
        .await
        .expect("состояние цепочки");
    let last = state
        .last
        .expect("сверка только что была - записи не может не быть");
    assert!(
        last.checked_at >= check.checked_at,
        "последняя сверка не может быть старше только что выполненной"
    );
    assert!(
        state.last_intact_at.is_some(),
        "успешная сверка обязана попасть и в «последнюю успешную»"
    );

    // Прежняя точка входа (гейт G15) и новый отчет считают одно и то же
    let status = tou_db::audit::verify_chain(&db)
        .await
        .expect("сверка цепочки");
    assert!(status.intact);
    assert_eq!(
        status.broken_at, None,
        "у целой цепочки места расхождения быть не может"
    );
}

/// INV-A01: результат сверки не переписывается, не удаляется и не подделывается.
#[tokio::test]
async fn inv_a01_chain_checks_are_append_only_and_not_forgeable() {
    let db = require_db!();

    // Правки перечислены отдельными вызовами, а не циклом по массиву:
    // текст запроса макросу нужен литералом - из переменной он его не увидит
    macro_rules! rejected {
        ($statement:literal) => {{
            let error = sqlx::query!($statement)
                .execute(&db)
                .await
                .expect_err("правка журнала сверок обязана быть отклонена")
                .to_string();
            assert!(
                error.contains("append-only")
                    || error.contains("INV-A01")
                    || error.contains("permission denied"),
                "{}: {error}",
                $statement
            );
        }};
    }

    rejected!(
        "UPDATE audit.chain_checks SET intact = true WHERE id = (SELECT max(id) FROM audit.chain_checks)"
    );
    rejected!("DELETE FROM audit.chain_checks WHERE id = (SELECT max(id) FROM audit.chain_checks)");
    // Отметка «цепочка цела» появляется только от настоящего пересчета:
    // права INSERT у роли приложения нет, писать может лишь SECURITY
    // DEFINER-функция audit.run_chain_check()
    rejected!("INSERT INTO audit.chain_checks (intact, entries) VALUES (true, 0)");
}
