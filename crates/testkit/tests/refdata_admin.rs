//! Ведение справочников ставок админом (T53, FR-1901, FR-202) против живой БД.
//!
//! Проверяется главное свойство справочника, а не форма запроса: правка не
//! переписывает историю. МРП правится по годам; коэффициент версионируется
//! датой вступления в силу, и версия «на будущее» не меняет то, что
//! применяется сегодня. Правка справочника - юридический факт и обязана
//! попадать в аудит (FR-1601, регламент А.5).
//!
//! Тест работает в откатываемой транзакции и потому пользуется вариантами
//! `*_on`: у роли приложения **нет права DELETE** на справочники - версия
//! множителя не удаляется, а добавляется, и это тот же рубеж, что FR-202.
//! Уборка «после» здесь невозможна в принципе, значит и следов оставаться
//! не должно.
//!
//! Перечень множителей и их опций закрыт Прил. 4 (`refdata.rate_options`,
//! внешний ключ), поэтому тест берет существующую пару, а не выдумывает свою.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use rust_decimal::Decimal;
use time::{Date, Duration};
use tou_db::refdata::{self, NewCoefficientVersion};
use uuid::Uuid;

async fn try_pool() -> Result<Option<tou_db::Db>, sqlx::Error> {
    // Пропуск без адреса допустим локально, но не в пайплайне (G2/G15):
    // молча пройденный интеграционный тест ничего не проверяет
    match tou_testkit::database_ready()
        .await
        .map_err(|e| sqlx::Error::Configuration(Box::new(e)))?
    {
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - справочники не проверялись");
                return;
            }
        }
    };
}

/// Синтетический год: в расчетах не участвует.
const YEAR: i32 = 2099;

/// Сегодня по часам сервера (`core.now()`, ADR-0005), а не процесса:
/// со сдвинутыми часами стенда даты разошлись бы, и тест сравнивал бы
/// свой день с чужим.
async fn today(tx: &mut sqlx::PgConnection) -> Result<Date, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT core.now()::date AS "day!""#)
        .fetch_one(tx)
        .await
}

/// Админ-актор и открытая транзакция от его имени: аудит-триггеры читают
/// `app.user_id`, а транзакция откатится вместе с записями аудита.
async fn admin_tx(
    db: &tou_db::Db,
) -> Result<(sqlx::Transaction<'static, sqlx::Postgres>, Uuid), sqlx::Error> {
    let mut tx = db.begin().await?;
    let tag = Uuid::now_v7().simple().to_string();

    let actor = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'argon2-заглушка', 'Т53 админ', now()) RETURNING id",
        format!("t53-admin-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    tou_db::set_actor(&mut tx, actor).await?;
    Ok((tx, actor))
}

/// Существующая пара «множитель × опция» из закрытого каталога Прил. 4.
async fn existing_option(conn: &mut sqlx::PgConnection) -> Result<(String, String), sqlx::Error> {
    let row = sqlx::query!(
        "SELECT coefficient, option_code FROM refdata.rate_options
         ORDER BY coefficient, option_code LIMIT 1"
    )
    .fetch_one(conn)
    .await?;
    Ok((row.coefficient, row.option_code))
}

/// FR-1901: величина МРП на год заводится и правится, правка идет в аудит.
#[tokio::test]
async fn mrp_is_set_and_audited() {
    let db = require_db!();
    let (mut tx, actor) = admin_tx(&db).await.expect("админ");

    let created = refdata::upsert_mrp_on(&mut tx, YEAR, Decimal::from(3932))
        .await
        .expect("МРП заведен");
    assert_eq!(created.year, YEAR);
    assert_eq!(created.amount, Decimal::from(3932));

    // У показателя одна величина на год: повторная запись правит значение
    let updated = refdata::upsert_mrp_on(&mut tx, YEAR, Decimal::from(4000))
        .await
        .expect("МРП поправлен");
    assert_eq!(updated.amount, Decimal::from(4000));

    let count = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM refdata.mrp WHERE year = $1"#,
        YEAR
    )
    .fetch_one(&mut *tx)
    .await
    .expect("перечень МРП");
    assert_eq!(count, 1, "год не задваивается");

    let audited = sqlx::query_scalar!(
        r#"SELECT count(*) AS "audited!" FROM audit.log
           WHERE table_name = 'refdata.mrp' AND actor_id = $1"#,
        actor
    )
    .fetch_one(&mut *tx)
    .await
    .expect("чтение аудита");
    assert!(audited >= 2, "правки справочника МРП не попали в audit.log");
}

/// FR-202: версия, вступающая в силу позже, не меняет расчет сегодня -
/// именно это делает прошлые расчеты неизменными.
#[tokio::test]
async fn future_version_does_not_change_today() {
    let db = require_db!();
    let (mut tx, _actor) = admin_tx(&db).await.expect("админ");
    let (coefficient, option_code) = existing_option(&mut tx).await.expect("опция Прил. 4");
    let day = today(&mut tx).await.expect("дата сервера");

    let effective_today = sqlx::query_scalar!(
        "SELECT value FROM refdata.rate_coefficients
         WHERE coefficient = $1 AND option_code = $2 AND effective_from <= current_date
         ORDER BY effective_from DESC LIMIT 1",
        coefficient,
        option_code
    )
    .fetch_one(&mut *tx)
    .await
    .expect("у опции каталога есть действующее значение");

    // Заведомо иная величина и дата в будущем: если бы «сегодня» считалось
    // по последней записи, а не по вступившей в силу, тест это увидел бы
    let id = refdata::add_coefficient_version_on(
        &mut tx,
        NewCoefficientVersion {
            coefficient: &coefficient,
            option_code: &option_code,
            label_ru: "Т53 проба",
            label_kk: None,
            label_en: None,
            value: Decimal::new(9999, 4),
            effective_from: day + Duration::days(365),
        },
    )
    .await
    .expect("версия на будущее")
    .expect("версии с этой датой еще не было");

    let after = sqlx::query_scalar!(
        "SELECT value FROM refdata.rate_coefficients
         WHERE coefficient = $1 AND option_code = $2 AND effective_from <= current_date
         ORDER BY effective_from DESC LIMIT 1",
        coefficient,
        option_code
    )
    .fetch_one(&mut *tx)
    .await
    .expect("действующее значение осталось");
    assert_eq!(
        after, effective_today,
        "версия из будущего не меняет расчет на сегодня"
    );

    // История сохранена целиком, «действует» помечена ровно одна версия
    let versions = sqlx::query!(
        r#"SELECT id, effective_from <= current_date AND id = first_value(id) OVER (
                    ORDER BY (effective_from <= current_date) DESC, effective_from DESC
                  ) AS "current!"
           FROM refdata.rate_coefficients
           WHERE coefficient = $1 AND option_code = $2"#,
        coefficient,
        option_code
    )
    .fetch_all(&mut *tx)
    .await
    .expect("версии опции");

    assert!(
        versions.len() >= 2,
        "прежняя версия остается в справочнике: {}",
        versions.len()
    );
    assert_eq!(
        versions.iter().filter(|row| row.current).count(),
        1,
        "действующей может быть только одна версия"
    );
    assert!(
        !versions.iter().any(|row| row.id == id && row.current),
        "версия из будущего не помечается действующей"
    );

    let audited = sqlx::query_scalar!(
        r#"SELECT count(*) AS "audited!" FROM audit.log
           WHERE table_name = 'refdata.rate_coefficients' AND row_id = $1"#,
        id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("чтение аудита");
    assert!(audited >= 1, "версия коэффициента не попала в audit.log");
}

/// G-137, FR-202: повторная запись с той же датой вступления отказывает и
/// прежнюю величину не трогает.
///
/// Версия справочника - история расчетов, а не поле формы: пока совпадение
/// по дате разворачивалось в `DO UPDATE`, вторая отправка формы молча
/// переписывала основание уже посчитанных ставок, и прежнего значения не
/// оставалось нигде, кроме аудита. Отказ дает админу единственный
/// осмысленный выход - другую дату начала действия.
#[tokio::test]
async fn same_effective_date_is_refused() {
    let db = require_db!();
    let (mut tx, _actor) = admin_tx(&db).await.expect("админ");
    let (coefficient, option_code) = existing_option(&mut tx).await.expect("опция Прил. 4");
    let day = today(&mut tx).await.expect("дата сервера");

    let mut version = NewCoefficientVersion {
        coefficient: &coefficient,
        option_code: &option_code,
        label_ru: "Т53 проба",
        label_kk: None,
        label_en: None,
        value: Decimal::new(15, 1),
        effective_from: day + Duration::days(730),
    };

    let first = refdata::add_coefficient_version_on(&mut tx, version)
        .await
        .expect("версия заведена")
        .expect("дата свободна");

    version.value = Decimal::new(17, 1);
    let second = refdata::add_coefficient_version_on(&mut tx, version)
        .await
        .expect("повтор разобран");

    assert!(
        second.is_none(),
        "версия на занятую дату не заводится - обработчик отвечает 409"
    );

    let value = sqlx::query_scalar!(
        "SELECT value FROM refdata.rate_coefficients WHERE id = $1",
        first
    )
    .fetch_one(&mut *tx)
    .await
    .expect("версия найдена");
    assert_eq!(
        value,
        Decimal::new(15, 1),
        "прежняя величина версии осталась нетронутой"
    );

    let rows = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM refdata.rate_coefficients
           WHERE coefficient = $1 AND option_code = $2 AND effective_from = $3"#,
        coefficient,
        option_code,
        version.effective_from
    )
    .fetch_one(&mut *tx)
    .await
    .expect("перечень версий");
    assert_eq!(rows, 1, "второй строки на ту же дату не появилось");
}

/// Лот, в снимке расчета которого стоит заданная величина МРП.
///
/// Лот заводится тестом, а не берется из данных стенда (G-021): тесты идут
/// против своей БД, и на пустой `SELECT id FROM core.lots LIMIT 1` не
/// находил ничего - сторож удаления оказывался «проверенным» на снимке,
/// которого не существует.
async fn lot_with_mrp_snapshot(
    tx: &mut sqlx::PgConnection,
    organizer: Uuid,
    mrp: Decimal,
) -> Result<Uuid, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let object = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', $1, 'адрес', 12.00) RETURNING id",
        format!("Т53 объект {tag}")
    )
    .fetch_one(&mut *tx)
    .await?;

    let tender = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id) VALUES ($1, 'draft', $2)
         RETURNING id",
        format!("Т53 тендер {tag}"),
        organizer
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query_scalar!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                base_rate_monthly, guarantee_fee, rate_calculation)
         VALUES ($1, 1, $2, 'офис', 12, 50000.00, 50000.00,
                 jsonb_build_object('inputs', jsonb_build_object('mrp', $3::text)))
         RETURNING id",
        tender,
        object,
        mrp.to_string()
    )
    .fetch_one(&mut *tx)
    .await
}

/// G-141, FR-201: год МРП удаляется, пока на нем ничего не стоит, и не
/// удаляется, когда его величина попала в замороженный снимок расчета
/// (FR-202).
///
/// Сторож стоит триггером таблицы: право удалять есть у роли приложения
/// целиком, и проверка в обработчике закрывала бы один путь из нескольких.
#[tokio::test]
async fn mrp_year_is_deleted_only_while_nothing_stands_on_it() {
    let db = require_db!();
    let (mut tx, actor) = admin_tx(&db).await.expect("админ");

    // Синтетический год и заведомо своя величина: снимков с ней нет
    let amount = Decimal::new(1_234_567, 2);
    refdata::upsert_mrp_on(&mut tx, YEAR, amount)
        .await
        .expect("МРП заведен");

    let free = sqlx::query!("DELETE FROM refdata.mrp WHERE year = $1", YEAR)
        .execute(&mut *tx)
        .await
        .expect("свободный год удаляется");
    assert_eq!(free.rows_affected(), 1);

    // Тот же год под снимком расчета лота уже не удаляется
    refdata::upsert_mrp_on(&mut tx, YEAR, amount)
        .await
        .expect("МРП заведен снова");
    let used = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM core.lots
           WHERE core.rate_snapshot_mrp(rate_calculation) = $1"#,
        amount
    )
    .fetch_one(&mut *tx)
    .await
    .expect("снимки лотов");
    assert_eq!(used, 0, "величина выбрана свободной от снимков");

    lot_with_mrp_snapshot(&mut tx, actor, amount)
        .await
        .expect("лот со снимком расчета");

    let refused = sqlx::query!("DELETE FROM refdata.mrp WHERE year = $1", YEAR)
        .execute(&mut *tx)
        .await;
    match refused {
        Ok(_) => panic!("год под снимком расчета удалять нельзя (FR-201, FR-202)"),
        Err(sqlx::Error::Database(err)) => assert_eq!(
            tou_domain::rule::RuleViolation::from_message(err.message()),
            Some(tou_domain::rule::RuleViolation::MrpYearInUse),
            "отказ обязан приезжать причиной перечня, а не безымянным P0001: {}",
            err.message()
        ),
        Err(other) => panic!("ожидался отказ правила, получено: {other}"),
    }
}

/// G-141: текущий год не удаляется и без единого снимка - по нему считаются
/// новые ставки (`refdata.current_mrp`, A-010).
#[tokio::test]
async fn the_current_mrp_year_is_never_deleted() {
    let db = require_db!();
    let (mut tx, _actor) = admin_tx(&db).await.expect("админ");

    let current_year = sqlx::query_scalar!(
        r#"SELECT extract(year FROM (core.now() AT TIME ZONE 'Asia/Almaty'))::int
             AS "year!""#
    )
    .fetch_one(&mut *tx)
    .await
    .expect("год по часам сервера");

    refdata::upsert_mrp_on(&mut tx, current_year, Decimal::new(7_654_321, 2))
        .await
        .expect("МРП текущего года заведен");

    let refused = sqlx::query!("DELETE FROM refdata.mrp WHERE year = $1", current_year)
        .execute(&mut *tx)
        .await;
    match refused {
        Ok(_) => panic!("текущий год удалять нельзя (FR-201)"),
        Err(sqlx::Error::Database(err)) => assert_eq!(
            tou_domain::rule::RuleViolation::from_message(err.message()),
            Some(tou_domain::rule::RuleViolation::MrpYearInUse),
        ),
        Err(other) => panic!("ожидался отказ правила, получено: {other}"),
    }
}

/// Перечень множителей и опций задан Прил. 4: выдуманная опция отклоняется
/// до вставки - тот же рубеж, что внешний ключ в БД.
#[tokio::test]
async fn unknown_option_is_not_in_the_catalog() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let (coefficient, option_code) = existing_option(&mut tx).await.expect("опция Прил. 4");
    drop(tx);

    assert!(
        refdata::rate_option_exists(&db, &coefficient, &option_code)
            .await
            .expect("проверка каталога")
    );
    assert!(
        !refdata::rate_option_exists(&db, &coefficient, "t53_made_up")
            .await
            .expect("проверка каталога"),
        "выдуманная опция не должна находиться в каталоге Прил. 4"
    );
}
