//! Способ подписания документа (T63, ТЗ § 2) против живой БД.
//!
//! ЭЦП вне периметра, поэтому подписанным считается документ с загруженным
//! сканом. Правило записано дважды - типом `domain::signing::SignatureStatus`
//! и триггером `core.sync_signature_status` - и тест сверяет их на всех
//! сочетаниях, чтобы половины не разошлись (как в паритете календаря G12).
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use tou_domain::signing::SignatureStatus;
use uuid::Uuid;

const SCAN: &str = "contracts/t63/scan.pdf";

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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - подпись не проверялась");
                return;
            }
        }
    };
}

/// Документ, у которого есть способ подписания. Правило одно на обе таблицы,
/// но имя таблицы в проверяемом по схеме запросе переменной быть не может -
/// перечисление разводит две формы одного запроса.
#[derive(Clone, Copy)]
enum Doc {
    Contract,
    Act,
}

/// Договор без скана: заготовка, к которой тест прикладывает и снимает скан.
async fn contract(tx: &mut sqlx::PgConnection) -> Result<Uuid, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let tenant = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'argon2-заглушка', 'Т63 наниматель', now()) RETURNING id",
        format!("t63-tenant-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let object = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Т63 помещение', 'г. Павлодар, ул. Тестовая, 63', 42)
         RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query_scalar!(
        "INSERT INTO core.contracts (object_id, tenant_id, monthly_rate, status)
         VALUES ($1, $2, 21000, 'draft') RETURNING id",
        object,
        tenant
    )
    .fetch_one(&mut *tx)
    .await
}

/// Способ подписания глазами БД. Значение разбирается доменным типом:
/// разошедшийся enum обязан падать, а не молча читаться строкой.
async fn status_of(
    tx: &mut sqlx::PgConnection,
    doc: Doc,
    id: Uuid,
) -> Result<SignatureStatus, sqlx::Error> {
    let raw = match doc {
        Doc::Contract => {
            sqlx::query_scalar!(
                r#"SELECT signature_status::text AS "status!"
                   FROM core.contracts WHERE id = $1"#,
                id
            )
            .fetch_one(&mut *tx)
            .await?
        }
        Doc::Act => {
            sqlx::query_scalar!(
                r#"SELECT signature_status::text AS "status!"
                   FROM core.acts WHERE id = $1"#,
                id
            )
            .fetch_one(&mut *tx)
            .await?
        }
    };
    raw.parse().map_err(|e| sqlx::Error::Decode(Box::new(e)))
}

async fn set_scan(
    tx: &mut sqlx::PgConnection,
    doc: Doc,
    id: Uuid,
    scan: Option<&str>,
) -> Result<(), sqlx::Error> {
    match doc {
        Doc::Contract => {
            sqlx::query!(
                "UPDATE core.contracts SET signed_scan_key = $2 WHERE id = $1",
                id,
                scan
            )
            .execute(&mut *tx)
            .await
        }
        Doc::Act => {
            sqlx::query!(
                "UPDATE core.acts SET signed_scan_key = $2 WHERE id = $1",
                id,
                scan
            )
            .execute(&mut *tx)
            .await
        }
    }
    .map(|_| ())
}

/// ТЗ § 2: свежий договор не подписан, скан делает его подписанным на бумаге,
/// снятие скана возвращает документ в неподписанное состояние.
#[tokio::test]
async fn scan_drives_paper_signature() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let id = contract(&mut tx).await.expect("договор");

    let status = status_of(&mut tx, Doc::Contract, id).await.expect("статус");
    assert_eq!(status, SignatureStatus::Unsigned);

    set_scan(&mut tx, Doc::Contract, id, Some(SCAN))
        .await
        .expect("скан приложен");
    let status = status_of(&mut tx, Doc::Contract, id).await.expect("статус");
    assert_eq!(status, SignatureStatus::Paper);

    set_scan(&mut tx, Doc::Contract, id, None)
        .await
        .expect("скан снят");
    let status = status_of(&mut tx, Doc::Contract, id).await.expect("статус");
    assert_eq!(status, SignatureStatus::Unsigned);
}

/// Электронная подпись сильнее бумажной: правка скана ее не снимает.
/// Значение ставит только провайдер подписи (в периметре его нет),
/// поэтому тест выставляет его напрямую - как это сделал бы адаптер.
#[tokio::test]
async fn electronic_signature_is_not_overwritten() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let id = contract(&mut tx).await.expect("договор");

    sqlx::query!(
        "UPDATE core.contracts SET signature_status = 'electronic' WHERE id = $1",
        id
    )
    .execute(&mut *tx)
    .await
    .expect("подпись провайдера");

    for scan in [Some(SCAN), None] {
        set_scan(&mut tx, Doc::Contract, id, scan)
            .await
            .expect("правка скана");
        let status = status_of(&mut tx, Doc::Contract, id).await.expect("статус");
        assert_eq!(
            status,
            SignatureStatus::Electronic,
            "скан {scan:?} не отменяет электронную подпись"
        );
    }
}

/// Паритет половин правила: домен и триггер дают одно и то же на всех
/// сочетаниях исходного статуса и наличия скана.
#[tokio::test]
async fn domain_and_trigger_agree() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let id = contract(&mut tx).await.expect("договор");

    for initial in SignatureStatus::ALL {
        for has_scan in [true, false] {
            sqlx::query!(
                "UPDATE core.contracts
                 SET signature_status = $2::text::core.signature_status, signed_scan_key = NULL
                 WHERE id = $1",
                id,
                initial.as_str()
            )
            .execute(&mut *tx)
            .await
            .expect("исходное состояние");

            // Триггер уже мог поправить статус при обнулении скана -
            // сравниваются одинаково подготовленные половины
            let before = status_of(&mut tx, Doc::Contract, id).await.expect("статус");
            set_scan(&mut tx, Doc::Contract, id, has_scan.then_some(SCAN))
                .await
                .expect("правка скана");

            let after = status_of(&mut tx, Doc::Contract, id).await.expect("статус");
            assert_eq!(
                after,
                before.with_scan(has_scan),
                "исходный {initial:?}, скан {has_scan}"
            );
        }
    }
}

/// Акт составляется только по зарегистрированному договору (FR-904, п. 126),
/// поэтому тест доводит договор до этого состояния - иначе проверялся бы не
/// способ подписания, а порядок актов.
async fn registered_contract(tx: &mut sqlx::PgConnection) -> Result<Uuid, sqlx::Error> {
    let id = contract(&mut *tx).await?;
    let tag = Uuid::now_v7().simple().to_string();

    // INV-115: наймодатель не подписывает договор без завершенной сверки -
    // одна закрытая позиция чек-листа делает состояние законным
    // Код позиции берется из справочника п. 113 (закрытый перечень, FK),
    // а не придумывается тестом
    sqlx::query!(
        "INSERT INTO core.contract_checklists (contract_id, item_code, checked_at)
         SELECT $1, code, now() FROM refdata.checklist_items ORDER BY code LIMIT 1",
        id
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        "UPDATE core.contracts
         SET status = 'active',
             lease_period = tstzrange(now(), now() + interval '365 days'),
             drafted_at = now(),
             handed_to_tenant_at = now(),
             tenant_signed_at = now(),
             documents_received_at = now(),
             landlord_signed_at = now(),
             reg_number = $2,
             registered_at = now()
         WHERE id = $1",
        id,
        format!("Д-Т63-{tag}")
    )
    .execute(&mut *tx)
    .await?;

    Ok(id)
}

/// Акт подчиняется тому же правилу: подпись - это загруженный скан.
#[tokio::test]
async fn acts_follow_the_same_rule() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let contract_id = registered_contract(&mut tx).await.expect("договор");

    let act = sqlx::query_scalar!(
        "INSERT INTO core.acts (contract_id, kind, act_date)
         VALUES ($1, 'handover', current_date) RETURNING id",
        contract_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("акт приема-передачи");

    let status = status_of(&mut tx, Doc::Act, act)
        .await
        .expect("статус акта");
    assert_eq!(status, SignatureStatus::Unsigned);

    set_scan(&mut tx, Doc::Act, act, Some("acts/t63/scan.pdf"))
        .await
        .expect("скан акта");
    let status = status_of(&mut tx, Doc::Act, act)
        .await
        .expect("статус акта");
    assert_eq!(status, SignatureStatus::Paper);
}
