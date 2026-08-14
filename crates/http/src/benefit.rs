//! Льготные схемы договоров особого порядка (М12, FR-1205, п. 95–96).
//!
//! Льгота применяется к договору вместе с подтверждением своих условий:
//! согласование Ученого совета (п. 95) и обучение спин-оффа (п. 96)
//! проверяются доменом и триггером (INV-095, INV-096). Расписание платы
//! по годам считает домен - интерфейс показывает его как есть.

use axum::extract::State;
use axum::http::StatusCode;
use garde::Validate as _;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::Date;
use tou_db::benefit::{self, BenefitError, GrantRecord};
use tou_domain::benefit::{Benefit, Conditions, YearPayment};
use tou_domain::money::Money;
use tou_domain::policy::{Action, Compound};
use tou_domain::special::BenefitScheme;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::dto::iso_date;
use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::request::{Json, Path};
use crate::state::AppState;
use tou_domain::rule::RuleViolation;

fn benefit_error(err: BenefitError) -> ApiError {
    match err {
        BenefitError::NotFound => ApiError::NotFound,
        BenefitError::Rejected(reason) => ApiError::RuleViolation(reason),
        BenefitError::Db(db) => db.into(),
    }
}

/// Льготная схема со своими параметрами (FR-1205, п. 95–96, Прил. 4).
#[derive(Debug, Serialize, ToSchema)]
pub struct BenefitSchemeDto {
    pub code: String,
    pub label_ru: String,
    pub label_kk: Option<String>,
    pub label_en: Option<String>,
    pub rule_ref: String,
    /// Есть ли расписание платы по годам (п. 95–96)
    pub has_schedule: bool,
    /// Доля ставки Прил. 4 со второго года найма, %
    #[schema(value_type = String, example = "50.00")]
    pub later_share_pct: Decimal,
    /// Нужно согласование Ученого совета (п. 95)
    pub requires_council: bool,
    /// Минимум кредитов обучения в семестр (п. 96)
    pub min_study_credits: i32,
    /// Квота стажировок (п. 95); величина - из Правил (Q-010)
    pub internship_quota: i32,
}

/// Каталог льготных схем (FR-1205).
#[utoipa::path(
    get,
    path = "/api/v1/refdata/benefit-schemes",
    tag = "benefit",
    responses((status = 200, description = "Льготные схемы п. 95–96", body = [BenefitSchemeDto]))
)]
pub async fn benefit_schemes(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<BenefitSchemeDto>>, ApiError> {
    // Закрытый перечень из Правил: его знает и заявитель, выбирая категорию
    user.require(Action::RefdataRead)?;

    let rows = benefit::list_schemes(&state.db).await?;
    Ok(Json(
        rows.into_iter()
            .map(|row| BenefitSchemeDto {
                code: row.code,
                label_ru: row.label_ru,
                label_kk: row.label_kk,
                label_en: row.label_en,
                rule_ref: row.rule_ref,
                has_schedule: row.has_schedule,
                later_share_pct: row.later_share_pct,
                requires_council: row.requires_council,
                min_study_credits: row.min_study_credits,
                internship_quota: row.internship_quota,
            })
            .collect(),
    ))
}

/// Плата за год найма по льготному расписанию (п. 95–96).
#[derive(Debug, Serialize, ToSchema)]
pub struct YearPaymentDto {
    pub year: i32,
    /// `communal_only` | `share` | `full`
    pub rule: String,
    /// Доля ставки Прил. 4, %; null - плата равна коммунальным расходам
    pub share_pct: Option<u32>,
    #[schema(value_type = String, example = "50000.00")]
    pub monthly: Decimal,
}

/// Льгота договора и расписание платы (FR-1205).
#[derive(Debug, Serialize, ToSchema)]
pub struct BenefitGrantDto {
    pub contract_id: Uuid,
    pub scheme: String,
    #[schema(value_type = String, example = "18000.00")]
    pub communal_monthly: Decimal,
    #[schema(value_type = String, example = "100000.00")]
    pub base_monthly: Decimal,
    pub council_decision: Option<String>,
    #[serde(with = "iso_date::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub council_date: Option<Date>,
    pub study_credits: i32,
    pub internships: i32,
    pub granted_by_name: Option<String>,
    /// Расписание платы на срок найма (п. 95–96)
    pub schedule: Vec<YearPaymentDto>,
}

fn payment_dto(payment: YearPayment) -> YearPaymentDto {
    YearPaymentDto {
        year: payment.year,
        rule: payment.rule.as_str().to_owned(),
        share_pct: payment.rule.share_pct(),
        monthly: payment.monthly.amount(),
    }
}

fn grant_dto(record: GrantRecord, months: i32) -> Result<BenefitGrantDto, ApiError> {
    let scheme: BenefitScheme = record
        .scheme
        .parse()
        .map_err(|_| ApiError::internal(std::io::Error::other("льготная схема")))?;
    let benefit = Benefit {
        scheme,
        internship_quota: record.internship_quota,
    };
    let schedule = benefit
        .schedule(
            months,
            Money::new(record.base_monthly),
            Money::new(record.communal_monthly),
        )
        .map_err(|err| ApiError::rule(RuleViolation::BenefitScheme, err.to_string()))?;

    Ok(BenefitGrantDto {
        contract_id: record.contract_id,
        scheme: record.scheme,
        communal_monthly: record.communal_monthly,
        base_monthly: record.base_monthly,
        council_decision: record.council_decision,
        council_date: record.council_date,
        study_credits: record.study_credits,
        internships: record.internships,
        granted_by_name: record.granted_by_name,
        schedule: schedule.into_iter().map(payment_dto).collect(),
    })
}

/// Льгота договора с расписанием платы (FR-1205, п. 95–96).
#[utoipa::path(
    get,
    path = "/api/v1/contracts/{id}/benefit",
    tag = "benefit",
    params(("id" = Uuid, Path, description = "Договор")),
    responses(
        (status = 200, description = "Льгота договора", body = BenefitGrantDto),
        (status = 404, description = "Льгота не применялась", body = crate::error::Problem),
    )
)]
pub async fn contract_benefit(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<BenefitGrantDto>, ApiError> {
    require_benefit_access(&user)?;

    let record = benefit::of_contract(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let months = contract_term_months(&state, id).await?;
    Ok(Json(grant_dto(record, months)?))
}

#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct GrantBenefitRequest {
    /// Код схемы: `educational_equipment` | `spin_off` | `social` | `none`
    #[garde(skip)]
    pub scheme: String,
    /// Коммунальные расходы за месяц - плата первого года (п. 95–96)
    #[garde(custom(non_negative))]
    #[schema(value_type = String, example = "18000.00")]
    pub communal_monthly: Decimal,
    /// Реквизиты решения Ученого совета (п. 95)
    #[garde(inner(length(chars, max = 300)))]
    pub council_decision: Option<String>,
    #[serde(default, with = "iso_date::option")]
    #[schema(value_type = Option<String>, format = Date)]
    #[garde(skip)]
    pub council_date: Option<Date>,
    /// Кредитов обучения в семестр (п. 96)
    #[garde(range(min = 0, max = 60))]
    pub study_credits: i32,
    /// Мест стажировок по квоте (п. 95)
    #[garde(range(min = 0, max = 1000))]
    pub internships: i32,
}

fn non_negative(value: &Decimal, _ctx: &()) -> garde::Result {
    if *value >= Decimal::ZERO {
        Ok(())
    } else {
        Err(garde::Error::new("сумма должна быть >= 0"))
    }
}

/// Применение льготы к договору (FR-1205, п. 95–96): условия схемы
/// проверяются доменом и триггером - без согласования Ученого совета
/// (INV-095) и без пяти кредитов спин-оффа (INV-096) льготы нет.
#[utoipa::path(
    post,
    path = "/api/v1/contracts/{id}/benefit",
    tag = "benefit",
    params(("id" = Uuid, Path, description = "Договор")),
    request_body = GrantBenefitRequest,
    responses(
        (status = 201, description = "Льгота применена", body = BenefitGrantDto),
        (status = 409, description = "Условия льготы не выполнены (п. 95–96)", body = crate::error::Problem),
        (status = 422, description = "Данные не прошли проверку", body = crate::error::Problem),
    )
)]
pub async fn grant_benefit(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<GrantBenefitRequest>,
) -> Result<(StatusCode, Json<BenefitGrantDto>), ApiError> {
    user.require(Action::ContractManage)?;
    body.validate()
        .map_err(|r| ApiError::Validation(r.to_string()))?;

    let scheme: BenefitScheme = body
        .scheme
        .parse()
        .map_err(|_| ApiError::Validation("льготная схема вне перечня (FR-1205)".to_owned()))?;

    // Условия п. 95–96 в домене; тот же отказ выдаст триггер БД
    let quota = benefit::list_schemes(&state.db)
        .await?
        .into_iter()
        .find(|row| row.code == scheme.as_str())
        .map(|row| row.internship_quota)
        .unwrap_or_default();
    Benefit {
        scheme,
        internship_quota: quota,
    }
    .check(Conditions {
        council_approved: body.council_decision.is_some(),
        study_credits: body.study_credits,
        internships: body.internships,
    })
    .map_err(|err| ApiError::rule(RuleViolation::BenefitScheme, err.to_string()))?;

    let record = benefit::grant(
        &state.db,
        user.id(),
        benefit::NewGrant {
            contract_id: id,
            scheme: scheme.as_str(),
            communal_monthly: body.communal_monthly,
            council_decision: body
                .council_decision
                .as_deref()
                .filter(|text| !text.trim().is_empty()),
            council_date: body.council_date,
            study_credits: body.study_credits,
            internships: body.internships,
        },
    )
    .await
    .map_err(benefit_error)?;

    let months = contract_term_months(&state, id).await?;
    Ok((StatusCode::CREATED, Json(grant_dto(record, months)?)))
}

/// Срок найма в месяцах: у инвестиционного договора - свой (п. 94),
/// у прочих - период найма договора; по умолчанию год.
async fn contract_term_months(state: &AppState, contract_id: Uuid) -> Result<i32, ApiError> {
    let months = sqlx::query_scalar!(
        r#"SELECT coalesce(
            (SELECT i.term_months FROM core.investment_contracts i
              WHERE i.contract_id = $1),
            (SELECT greatest(1, (extract(epoch FROM (upper(c.lease_period) - lower(c.lease_period)))
                                 / 2629800)::int)
               FROM core.contracts c WHERE c.id = $1 AND c.lease_period IS NOT NULL),
            12) AS "months""#,
        contract_id
    )
    .fetch_one(&state.db)
    .await?;
    Ok(months.unwrap_or(12))
}

/// Льготу применяет тот, кто ведет договор; видят ее и те, кто ведет особый
/// порядок - состав прав описан в домене (`Compound::BENEFIT_READ`).
fn require_benefit_access(user: &CurrentUser) -> Result<(), ApiError> {
    user.require_any(Compound::BENEFIT_READ)
}
