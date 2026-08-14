//! Договорный конвейер (М9, FR-901–902, FR-905): договор из итогов торгов,
//! чек-лист сверки документов, шаги подписания и регистрация.
//!
//! Существенные условия переносятся из итогов торгов один раз и дальше
//! неизменяемы (триггер `freeze_terms`), подпись наймодателя блокируется
//! без завершенной сверки (INV-115), а каждый шаг закрывает свой срок
//! и открывает следующий (FR-1702, п. 110–115).

use rust_decimal::Decimal;
use time::OffsetDateTime;
use tou_domain::contract::{Progress, Stage};
use tou_domain::obligation::ObligationAction;
use tou_domain::rule::{RuleRejection, RuleViolation};
use uuid::Uuid;

use crate::Db;
use crate::obligations::Subject;

#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    #[error("договор не найден")]
    NotFound,
    /// Правило конвейера (порядок шагов, INV-115, FR-901, FR-905)
    #[error("{0}")]
    Rejected(RuleRejection),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

impl From<tou_domain::contract::StageError> for ContractError {
    fn from(err: tou_domain::contract::StageError) -> Self {
        // Незавершенная сверка - отдельное правило (INV-115): участнику оно
        // говорит, что делать, а «шаг не в том порядке» - не говорит.
        let rule = match &err {
            tou_domain::contract::StageError::ChecklistIncomplete => {
                RuleViolation::DocumentCheckIncomplete
            }
            tou_domain::contract::StageError::AlreadyDone(_)
            | tou_domain::contract::StageError::OutOfOrder { .. } => {
                RuleViolation::ContractStageOrder
            }
        };
        ContractError::Rejected(RuleRejection::new(rule, err.to_string()))
    }
}

fn map_rule(err: sqlx::Error) -> ContractError {
    if let sqlx::Error::Database(db_err) = &err
        && matches!(
            db_err.code().as_deref(),
            Some("P0001") | Some("23514") | Some("23503") | Some("23505") | Some("23P01")
        )
    {
        return ContractError::Rejected(crate::rule::rejection(db_err.as_ref()));
    }
    ContractError::Db(err)
}

pub struct ContractRecord {
    pub id: Uuid,
    pub tender_id: Option<Uuid>,
    pub lot_id: Option<Uuid>,
    pub lot_seq: Option<i32>,
    pub object_name: String,
    pub tenant_id: Uuid,
    pub tenant_name: String,
    pub status: String,
    /// Победитель или участник № 2 после уклонения (FR-903, п. 117)
    pub place: String,
    /// Уклонение по этому договору зафиксировано (п. 116)
    pub evaded: bool,
    pub monthly_rate: Decimal,
    pub lease_months: Option<i32>,
    pub reg_number: Option<String>,
    pub registered_at: Option<OffsetDateTime>,
    pub signed_scan_key: Option<String>,
    /// Способ подписания (ТЗ § 2): выводится из наличия скана триггером
    pub signature_status: String,
    pub pdf_key: Option<String>,
    pub drafted_at: Option<OffsetDateTime>,
    pub handed_to_tenant_at: Option<OffsetDateTime>,
    pub tenant_signed_at: Option<OffsetDateTime>,
    pub documents_received_at: Option<OffsetDateTime>,
    pub checklist_done_at: Option<OffsetDateTime>,
    pub landlord_signed_at: Option<OffsetDateTime>,
    pub copy_sent_at: Option<OffsetDateTime>,
}

impl ContractRecord {
    /// Пройденные шаги конвейера в терминах домена (FR-902).
    pub fn progress(&self) -> Progress {
        let current = [
            (Stage::Registered, self.registered_at),
            (Stage::CopySent, self.copy_sent_at),
            (Stage::LandlordSigned, self.landlord_signed_at),
            (Stage::ChecklistCompleted, self.checklist_done_at),
            (Stage::DocumentsReceived, self.documents_received_at),
            (Stage::TenantSigned, self.tenant_signed_at),
            (Stage::HandedToTenant, self.handed_to_tenant_at),
            (Stage::Drafted, self.drafted_at),
        ]
        .into_iter()
        .find(|(_, at)| at.is_some())
        .map(|(stage, _)| stage);

        Progress {
            current,
            checklist_complete: self.checklist_done_at.is_some(),
        }
    }
}

/// Выборка договора: общий список столбцов + хвост запроса.
///
/// `!` стоит там, где планировщик считает выражение потенциально NULL,
/// хотя оно таким быть не может: приведения `::text` над NOT NULL-столбцами
/// и `EXISTS`. `lot_seq` получает `?` - лот подтягивается LEFT JOIN'ом,
/// а `core.lots.seq` - NOT NULL, и nullability sqlx выводит по столбцу,
/// а не по виду соединения.
macro_rules! contract_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            ContractRecord,
            r#"SELECT c.id, c.tender_id, c.lot_id, l.seq AS "lot_seq?",
                      o.name AS object_name, c.tenant_id, u.full_name AS tenant_name,
                      c.status::text AS "status!", c.place::text AS "place!",
                      EXISTS (SELECT 1 FROM core.evasions e
                              WHERE e.contract_id = c.id) AS "evaded!",
                      c.monthly_rate, c.lease_months,
                      c.reg_number, c.registered_at, c.signed_scan_key, c.pdf_key,
                      c.signature_status::text AS "signature_status!",
                      c.drafted_at, c.handed_to_tenant_at, c.tenant_signed_at,
                      c.documents_received_at, c.checklist_done_at,
                      c.landlord_signed_at, c.copy_sent_at
               FROM core.contracts c
               JOIN core.objects o ON o.id = c.object_id
               JOIN core.users u ON u.id = c.tenant_id
               LEFT JOIN core.lots l ON l.id = c.lot_id"# + $tail
            $(, $arg)*
        )
    };
}

pub async fn get(db: &Db, id: Uuid) -> Result<Option<ContractRecord>, sqlx::Error> {
    contract_query!(" WHERE c.id = $1", id)
        .fetch_optional(db)
        .await
}

/// Договоры тендера (кабинет организатора): по одному на лот.
pub async fn list_for_tender(db: &Db, tender_id: Uuid) -> Result<Vec<ContractRecord>, sqlx::Error> {
    contract_query!(" WHERE c.tender_id = $1 ORDER BY l.seq", tender_id)
        .fetch_all(db)
        .await
}

/// Договоры нанимателя (кабинет участника).
///
/// Курсора нет: договоров у одного нанимателя единицы - выборку ограничивает
/// сама предметная область. Потолок остается защитой от невероятного, но
/// молчать о нем нельзя: наниматель не должен гадать, все ли договоры видит.
pub async fn list_for_tenant(
    db: &Db,
    tenant_id: Uuid,
) -> Result<crate::Page<ContractRecord>, sqlx::Error> {
    let rows = contract_query!(
        " WHERE c.tenant_id = $1 ORDER BY c.created_at DESC LIMIT $2",
        tenant_id,
        crate::probe_limit(crate::MAX_ROWS)
    )
    .fetch_all(db)
    .await?;
    let page = crate::Page::probe(rows, crate::MAX_ROWS);
    crate::warn_if_truncated(page.truncated, "contracts::list_for_tenant");
    Ok(page)
}

/// Составление договора по итогам торгов (FR-901, п. 108, 110).
///
/// Условия берутся из победившей ставки и лота - вызывающий их не задает.
/// Чек-лист сверки формируется сразу по перечню п. 113 для вида заявителя,
/// срок возврата подписанного экземпляра (п. 111) пойдет после передачи.
/// После уклонения победителя (FR-903) договор составляется с участником
/// № 2 и на его ставку - место тоже выводится из фактов, а не задается.
pub async fn draft_from_auction(
    db: &Db,
    actor: Uuid,
    lot_id: Uuid,
) -> Result<ContractRecord, ContractError> {
    crate::with_actor(db, actor, async |tx| {
        // Прекращенный уклонением договор места не занимает (п. 117)
        if let Some(existing) = contract_query!(
            " WHERE c.lot_id = $1 AND c.status NOT IN ('terminated', 'cancelled')",
            lot_id
        )
        .fetch_optional(&mut *tx)
        .await?
        {
            return Ok(existing);
        }

        // Победитель и его ставка - из завершенных торгов (FR-606): БД уже
        // проверила, что сумма победителя совпадает с реальной ставкой.
        // Если победитель уклонился, право на договор у участника № 2 (п. 117).
        let row = sqlx::query!(
            r#"SELECT a.winner_application_id, a.winner_amount,
                    a.runner_up_application_id, a.runner_up_amount,
                    l.tender_id, l.object_id, l.lease_months,
                    EXISTS (SELECT 1 FROM core.evasions e
                            WHERE e.lot_id = l.id AND e.place = 'winner') AS "winner_evaded!",
                    EXISTS (SELECT 1 FROM core.evasions e
                            WHERE e.lot_id = l.id AND e.place = 'runner_up')
                      AS "runner_up_evaded!",
                    -- `?`: протокол итогов приходит LEFT JOIN'ом, а `id` -
                    -- NOT NULL, и sqlx выводит nullability по столбцу
                    p.id AS "protocol_id?"
             FROM core.auctions a
             JOIN core.lots l ON l.id = a.lot_id
             LEFT JOIN core.protocols p ON p.tender_id = l.tender_id AND p.kind = 'results'
             WHERE a.lot_id = $1 AND a.status = 'finished'"#,
            lot_id
        )
        .fetch_optional(&mut *tx)
        .await?;

        let row = row.ok_or_else(|| {
            ContractError::Rejected(RuleRejection::new(
                RuleViolation::ContractConclusion,
                "договор составляется по завершенным торгам лота (п. 108)",
            ))
        })?;

        let winner_evaded = row.winner_evaded;
        // Третьего места Правила не знают: после уклонения № 2 договор по лоту
        // не составляется, тендер идет к основанию п. 81.4 (FR-801)
        if row.runner_up_evaded {
            return Err(ContractError::Rejected(RuleRejection::new(
                RuleViolation::ContractConclusion,
                "победитель и участник № 2 уклонились - договор по лоту не составляется, \
                 тендер признается несостоявшимся (п. 81.4, 117)",
            )));
        }
        let place = if winner_evaded { "runner_up" } else { "winner" };
        let (application, amount): (Option<Uuid>, Option<Decimal>) = if winner_evaded {
            (row.runner_up_application_id, row.runner_up_amount)
        } else {
            (row.winner_application_id, row.winner_amount)
        };
        let (winner_application_id, monthly_rate) = match (application, amount) {
            (Some(application), Some(amount)) => (application, amount),
            _ if winner_evaded => {
                return Err(ContractError::Rejected(RuleRejection::new(
                    RuleViolation::ContractConclusion,
                    "по лоту нет участника № 2 - договор не составляется, тендер идет \
                     к признанию несостоявшимся (п. 81.4, 117)",
                )));
            }
            _ => {
                return Err(ContractError::Rejected(RuleRejection::new(
                    RuleViolation::ContractConclusion,
                    "по лоту нет победителя - договор не составляется (п. 74)",
                )));
            }
        };

        // Наниматель и вид заявителя - из заявки стороны договора (п. 113)
        let applicant = sqlx::query!(
            r#"SELECT participant_id, applicant_kind::text AS "applicant_kind!"
             FROM core.applications WHERE id = $1"#,
            winner_application_id
        )
        .fetch_one(&mut *tx)
        .await?;

        let tender_id = row.tender_id;
        let object_id = row.object_id;
        let lease_months = row.lease_months;
        let tenant_id = applicant.participant_id;
        let applicant_kind = applicant.applicant_kind;
        let protocol_id = row.protocol_id;

        // `$7::text::core.auction_place`: место приходит строкой, приведение
        // к перечислению делает БД
        let contract_id = sqlx::query_scalar!(
            "INSERT INTO core.contracts
               (tender_id, lot_id, object_id, tenant_id, protocol_id, winner_application_id,
                place, status, monthly_rate, lease_months, drafted_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7::text::core.auction_place, 'draft', $8, $9,
                     core.now())
             RETURNING id",
            tender_id,
            lot_id,
            object_id,
            tenant_id,
            protocol_id,
            winner_application_id,
            place,
            monthly_rate,
            lease_months
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_rule)?;

        // Чек-лист сверки (п. 113): позиции для вида заявителя и общие
        sqlx::query!(
            "INSERT INTO core.contract_checklists (contract_id, item_code)
             SELECT $1, i.code FROM refdata.checklist_items i
             WHERE i.applicant_kind IS NULL OR i.applicant_kind::text = $2
             ON CONFLICT DO NOTHING",
            contract_id,
            applicant_kind
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;

        // Срок составления закрыт фактом (п. 110), дальше - очередь победителя
        crate::obligations::complete(
            &mut *tx,
            ObligationAction::ContractDraft,
            Subject::tender(tender_id),
        )
        .await?;

        fetch(&mut *tx, contract_id).await
    })
    .await
}

async fn fetch(tx: &mut sqlx::PgConnection, id: Uuid) -> Result<ContractRecord, ContractError> {
    contract_query!(" WHERE c.id = $1", id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ContractError::NotFound)
}

pub struct ChecklistRow {
    pub item_code: String,
    pub label_ru: String,
    pub rule_ref: String,
    pub checked_at: Option<OffsetDateTime>,
    pub checked_by_name: Option<String>,
}

/// Выборка чек-листа договора: один и тот же запрос идет и от пула,
/// и из транзакции отметки позиции.
///
/// `checked_by_name` получает `?`: имя приходит `LEFT JOIN`'ом, а
/// `core.users.full_name` - NOT NULL, и без аннотации sqlx вывел бы non-null.
macro_rules! checklist_query {
    ($contract_id:expr) => {
        sqlx::query_as!(
            ChecklistRow,
            r#"SELECT cl.item_code, i.label_ru, i.rule_ref, cl.checked_at,
                    u.full_name AS "checked_by_name?"
             FROM core.contract_checklists cl
             JOIN refdata.checklist_items i ON i.code = cl.item_code
             LEFT JOIN core.users u ON u.id = cl.checked_by
             WHERE cl.contract_id = $1
             ORDER BY i.seq"#,
            $contract_id
        )
    };
}

/// Чек-лист сверки документов договора (п. 113).
pub async fn checklist(db: &Db, contract_id: Uuid) -> Result<Vec<ChecklistRow>, sqlx::Error> {
    checklist_query!(contract_id).fetch_all(db).await
}

/// Отметка позиции сверки (п. 113). Снятие отметки возможно, пока договор
/// не подписан наймодателем: после подписания сверка - часть основания.
pub async fn check_item(
    db: &Db,
    actor: Uuid,
    contract_id: Uuid,
    item_code: &str,
    checked: bool,
) -> Result<Vec<ChecklistRow>, ContractError> {
    crate::with_actor(db, actor, async |tx| {
        let signed = sqlx::query_scalar!(
            "SELECT landlord_signed_at FROM core.contracts WHERE id = $1",
            contract_id
        )
        .fetch_optional(&mut *tx)
        .await?;
        match signed {
            None => return Err(ContractError::NotFound),
            Some(Some(_)) => {
                return Err(ContractError::Rejected(RuleRejection::new(
                    RuleViolation::DocumentCheckIncomplete,
                    "договор подписан наймодателем - сверка закрыта (п. 113, 115)",
                )));
            }
            Some(None) => {}
        }

        let updated = sqlx::query!(
            "UPDATE core.contract_checklists
             SET checked_at = CASE WHEN $3 THEN core.now() END,
                 checked_by = CASE WHEN $3 THEN $4::uuid END
             WHERE contract_id = $1 AND item_code = $2",
            contract_id,
            item_code,
            checked,
            actor
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;

        if updated.rows_affected() == 0 {
            return Err(ContractError::Rejected(RuleRejection::new(
                RuleViolation::DocumentCheckIncomplete,
                "позиция вне перечня сверки этого договора (п. 113)",
            )));
        }

        // Отметка «сверка завершена» держится фактом: все позиции отмечены
        sqlx::query!(
            "UPDATE core.contracts c
             SET checklist_done_at = CASE
                   WHEN NOT EXISTS (SELECT 1 FROM core.contract_checklists cl
                                    WHERE cl.contract_id = c.id AND cl.checked_at IS NULL)
                   THEN coalesce(c.checklist_done_at, core.now())
                 END
             WHERE c.id = $1",
            contract_id
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;

        checklist_query!(contract_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(map_rule)
    })
    .await
}

/// Шаг конвейера (FR-902, п. 110–115): порядок проверяет домен, INV-115 -
/// БД. Каждый шаг закрывает свой срок и ставит следующий.
pub async fn advance(
    db: &Db,
    actor: Uuid,
    contract_id: Uuid,
    stage: Stage,
) -> Result<ContractRecord, ContractError> {
    let record = get(db, contract_id).await?.ok_or(ContractError::NotFound)?;
    record.progress().check(stage)?;

    if stage == Stage::Registered {
        return Err(ContractError::Rejected(RuleRejection::new(
            RuleViolation::ContractRegistration,
            REGISTRATION_IS_SEPARATE,
        )));
    }

    crate::with_actor(db, actor, async |tx| {
        // Имя колонки в SQL подставлять нельзя - оно не биндится параметром,
        // а значит запрос собирался бы строкой. Поэтому ветвление дает целый
        // запрос: каждый проверен по схеме (T46), а подставить сюда что-то
        // извне невозможно по построению
        let done =
            match stage {
                Stage::Drafted => {
                    sqlx::query!(
                        "UPDATE core.contracts SET drafted_at = core.now() WHERE id = $1",
                        contract_id
                    )
                    .execute(&mut *tx)
                    .await
                }
                Stage::HandedToTenant => {
                    sqlx::query!(
                        "UPDATE core.contracts SET handed_to_tenant_at = core.now() WHERE id = $1",
                        contract_id
                    )
                    .execute(&mut *tx)
                    .await
                }
                Stage::TenantSigned => {
                    sqlx::query!(
                        "UPDATE core.contracts SET tenant_signed_at = core.now() WHERE id = $1",
                        contract_id
                    )
                    .execute(&mut *tx)
                    .await
                }
                Stage::DocumentsReceived => sqlx::query!(
                    "UPDATE core.contracts SET documents_received_at = core.now() WHERE id = $1",
                    contract_id
                )
                .execute(&mut *tx)
                .await,
                Stage::ChecklistCompleted => {
                    sqlx::query!(
                        "UPDATE core.contracts SET checklist_done_at = core.now() WHERE id = $1",
                        contract_id
                    )
                    .execute(&mut *tx)
                    .await
                }
                Stage::LandlordSigned => {
                    sqlx::query!(
                        "UPDATE core.contracts SET landlord_signed_at = core.now() WHERE id = $1",
                        contract_id
                    )
                    .execute(&mut *tx)
                    .await
                }
                Stage::CopySent => {
                    sqlx::query!(
                        "UPDATE core.contracts SET copy_sent_at = core.now() WHERE id = $1",
                        contract_id
                    )
                    .execute(&mut *tx)
                    .await
                }
                // Отсечена выше: у регистрации своя операция с номером журнала
                Stage::Registered => {
                    return Err(ContractError::Rejected(RuleRejection::new(
                        RuleViolation::ContractRegistration,
                        REGISTRATION_IS_SEPARATE,
                    )));
                }
            };
        done.map_err(map_rule)?;

        // Сроки конвейера: пройденный шаг закрывает свой срок, следующий -
        // открывает очередной (FR-1702, п. 110–115)
        let subject = Subject {
            contract_id: Some(contract_id),
            ..Default::default()
        };
        if let Some(done) = obligation_of(stage) {
            crate::obligations::complete(&mut *tx, done, subject).await?;
        }
        if let Some(next) = stage.next().and_then(obligation_of) {
            crate::obligations::schedule(&mut *tx, next, subject).await?;
        }

        fetch(&mut *tx, contract_id).await
    })
    .await
}

/// Регистрация из конвейера недоступна: ей нужен номер журнала (FR-905).
const REGISTRATION_IS_SEPARATE: &str =
    "регистрация выполняется отдельной операцией с номером журнала (FR-905)";

/// Срок, который закрывает этот шаг (у сверки и передачи экземпляра
/// собственного срока нет - они внутри шагов п. 112 и 115).
fn obligation_of(stage: Stage) -> Option<ObligationAction> {
    match stage {
        Stage::HandedToTenant => Some(ObligationAction::TenantSign),
        Stage::TenantSigned => Some(ObligationAction::TenantDocuments),
        Stage::DocumentsReceived | Stage::ChecklistCompleted => {
            Some(ObligationAction::LandlordSign)
        }
        Stage::LandlordSigned => Some(ObligationAction::ContractHandover),
        _ => None,
    }
}

/// Регистрация договора в журнале (FR-905, п. 126): дата регистрации -
/// дата заключения, период найма начинает действовать (INV-DB-02).
pub async fn register(
    db: &Db,
    actor: Uuid,
    contract_id: Uuid,
    reg_number: &str,
) -> Result<ContractRecord, ContractError> {
    crate::with_actor(db, actor, async |tx| {
        register_on(tx, actor, contract_id, reg_number).await
    })
    .await
}

/// То же в транзакции вызывающего - вариант `*_on` (арх. v3 § 6).
pub async fn register_on(
    tx: &mut sqlx::PgConnection,
    _actor: Uuid,
    contract_id: Uuid,
    reg_number: &str,
) -> Result<ContractRecord, ContractError> {
    if reg_number.trim().is_empty() {
        return Err(ContractError::Rejected(RuleRejection::new(
            RuleViolation::ContractRegistration,
            "номер в журнале регистрации договоров обязателен (п. 126)",
        )));
    }

    {
        // Период найма считается от даты заключения на срок лота: он же
        // защищает объект от пересекающихся аренд (INV-DB-02)
        sqlx::query!(
            "UPDATE core.contracts
             SET reg_number = $2, registered_at = core.now(), status = 'signing',
                 lease_period = tstzrange(core.now(),
                                          core.now() + make_interval(months => lease_months), '[)')
             WHERE id = $1 AND registered_at IS NULL",
            contract_id,
            reg_number.trim()
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;

        // Заключение договора открывает депозитный счет и срок внесения
        // депозита: он равен месячной плате и вносится за 10 рабочих дней
        // (FR-1003, п. 132). Счет открывается здесь, а не по факту первого
        // платежа: до внесения он показывает нулевой баланс и открытый срок,
        // а не отсутствие обязанности
        let tenant_id = sqlx::query_scalar!(
            "SELECT tenant_id FROM core.contracts WHERE id = $1",
            contract_id
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_rule)?;
        crate::ledger::open_deposit_account_on(&mut *tx, contract_id, tenant_id)
            .await
            .map_err(|err| match err {
                crate::ledger::LedgerError::Db(db) => ContractError::Db(db),
                other => ContractError::Rejected(RuleRejection::new(
                    RuleViolation::ContractDeposit,
                    other.to_string(),
                )),
            })?;
        crate::obligations::schedule(
            &mut *tx,
            ObligationAction::DepositPayment,
            crate::obligations::Subject::contract(contract_id),
        )
        .await?;

        fetch(&mut *tx, contract_id).await
    }
}

/// Скан подписанного экземпляра (FR-905, без ЭЦП).
pub async fn attach_scan(
    db: &Db,
    actor: Uuid,
    contract_id: Uuid,
    key: &str,
) -> Result<ContractRecord, ContractError> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query!(
            "UPDATE core.contracts SET signed_scan_key = $2 WHERE id = $1",
            contract_id,
            key
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;
        fetch(&mut *tx, contract_id).await
    })
    .await
}

/// Ключ PDF договора (печатная форма Прил. 5).
pub async fn attach_pdf(
    db: &Db,
    actor: Uuid,
    contract_id: Uuid,
    key: &str,
) -> Result<(), ContractError> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query!(
            "UPDATE core.contracts SET pdf_key = $2 WHERE id = $1",
            contract_id,
            key
        )
        .execute(&mut *tx)
        .await
        .map(|_| ())
        .map_err(map_rule)
    })
    .await
}
