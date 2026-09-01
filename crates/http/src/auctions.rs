//! Онлайн-торги: комната лота (М6, FR-601–603, 606).
//!
//! REST задает состояние комнаты и принимает ставки, WS раздает ленту всем
//! присутствующим (п. 65). Сервер - единственный источник времени и порядка:
//! `ends_at` и `seq` приходят из БД, клиент только отображает обратный отсчет
//! (FR-602). Ставка идемпотентна по клиентскому `id` - реконнект не создает
//! дубля (NFR-05).

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxumPath, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::BroadcastStream;
use tou_db::auction_turns::{self, CircleError};
use tou_db::auctions::{
    self, AuctionRecord, BidError, BidRecord, NewBid, ScheduleError, TransitionError,
};
use tou_domain::auction::{self, BidStepPercent};
use tou_domain::money::Money;
use tou_domain::policy::{Action, is_allowed};
use tou_domain::role::Role;
use tou_domain::turn::Progress;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::request::{Json, Path};
use crate::state::AppState;
use tou_domain::rule::RuleViolation;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AuctionDto {
    pub id: Uuid,
    pub lot_id: Uuid,
    pub tender_id: Uuid,
    pub lot_seq: i32,
    pub lot_purpose: String,
    /// `scheduled` | `running` | `finished` | `cancelled`
    pub status: String,
    /// Старт = максимум первоначальных предложений допущенных (INV-062)
    #[schema(value_type = String, example = "55000")]
    pub starting_bid: Decimal,
    /// Процент шага, выбранный секретарем; минимум 5 % (Q-019)
    #[schema(value_type = String, example = "7.5")]
    pub bid_step_percent: Decimal,
    /// Денежный шаг, зафиксированный при создании комнаты
    #[schema(value_type = String, example = "2750")]
    pub bid_step: Decimal,
    /// Текущий максимум ленты; до первой ставки - `null`
    #[schema(value_type = Option<String>, example = "57750")]
    pub current_max: Option<Decimal>,
    /// Минимально допустимая следующая ставка (INV-063)
    #[schema(value_type = String, example = "57750")]
    pub min_next_bid: Decimal,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub started_at: Option<OffsetDateTime>,
    /// Момент окончания по часам сервера (FR-602)
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub ends_at: Option<OffsetDateTime>,
    /// Продление уже израсходовано (INV-066)
    pub extended_once: bool,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub finished_at: Option<OffsetDateTime>,
    /// Завершены досрочно при общем согласии (п. 67)
    pub finished_early: bool,
    pub winner_application_id: Option<Uuid>,
    #[schema(value_type = Option<String>, example = "68750")]
    pub winner_amount: Option<Decimal>,
    pub runner_up_application_id: Option<Uuid>,
    #[schema(value_type = Option<String>, example = "66000")]
    pub runner_up_amount: Option<Decimal>,
}

impl AuctionDto {
    fn new(record: AuctionRecord, current_max: Option<Decimal>) -> Self {
        let min_next_bid = auction::min_next_bid(
            Money::new(record.starting_bid),
            current_max.map(Money::new),
            Money::new(record.bid_step),
        )
        .amount();

        Self {
            id: record.id,
            lot_id: record.lot_id,
            tender_id: record.tender_id,
            lot_seq: record.lot_seq,
            lot_purpose: record.lot_purpose,
            status: record.status,
            starting_bid: record.starting_bid,
            bid_step_percent: record.bid_step_percent,
            bid_step: record.bid_step,
            current_max,
            min_next_bid,
            started_at: record.started_at,
            ends_at: record.ends_at,
            extended_once: record.extended_once,
            finished_at: record.finished_at,
            finished_early: record.finished_early,
            winner_application_id: record.winner_application_id,
            winner_amount: record.winner_amount,
            runner_up_application_id: record.runner_up_application_id,
            runner_up_amount: record.runner_up_amount,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BidDto {
    pub id: Uuid,
    pub application_id: Uuid,
    pub applicant_name: String,
    #[schema(value_type = String, example = "57750")]
    pub amount: Decimal,
    /// Порядковый номер ставки, назначенный сервером (курсор реконнекта)
    pub seq: i64,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub placed_at: OffsetDateTime,
}

impl From<BidRecord> for BidDto {
    fn from(record: BidRecord) -> Self {
        Self {
            id: record.id,
            application_id: record.application_id,
            applicant_name: record.applicant_name,
            amount: record.amount,
            seq: record.seq,
            placed_at: record.placed_at,
        }
    }
}

/// Участник круга торгов (FR-604): очередность и состояние.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CircleParticipantDto {
    pub application_id: Uuid,
    pub applicant_name: String,
    /// Место в очередности - номер заявки в журнале регистрации
    pub turn_order: i32,
    /// `active` - торгуется, `passed` - выбыл (п. 65), `absent` - не явился (п. 70)
    pub status: String,
    /// Первоначальное предложение (Прил. 9): оглашается при неявке
    #[schema(value_type = String, example = "55000")]
    pub initial_price: Decimal,
}

impl From<auction_turns::ParticipantRow> for CircleParticipantDto {
    fn from(row: auction_turns::ParticipantRow) -> Self {
        Self {
            application_id: row.application_id,
            applicant_name: row.applicant_name,
            turn_order: row.turn_order,
            status: row.status.as_str().to_owned(),
            initial_price: row.initial_price,
        }
    }
}

/// Снимок комнаты: состояние + вся лента (FR-603) + место зрителя в ней.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AuctionRoomDto {
    pub auction: AuctionDto,
    pub bids: Vec<BidDto>,
    /// Допущенная заявка зрителя на этот лот; `null` - зритель не торгуется
    pub my_application_id: Option<Uuid>,
    /// Круг торгов в порядке очередности (FR-604)
    pub participants: Vec<CircleParticipantDto>,
    /// Чей сейчас ход; `null` - круг не начат либо торги окончены
    pub current_turn_application_id: Option<Uuid>,
    /// Часы сервера на момент ответа: клиент считает остаток по ним (FR-602)
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub server_time: OffsetDateTime,
}

/// Событие комнаты в WS-ленте; `type` - дискриминант на проводе.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RoomEvent {
    /// Новая ставка (FR-603): лента и минимум обновляются у всех
    Bid { bid: BidDto, auction: AuctionDto },
    /// Старт, продление, завершение - состояние целиком
    State { auction: AuctionDto },
    /// Смена хода или состава круга (FR-604–605)
    Turn {
        participants: Vec<CircleParticipantDto>,
        current_turn_application_id: Option<Uuid>,
    },
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ScheduleAuctionRequest {
    /// Процент от стартовой ставки: от 5 (Q-019) до 100 - шаг больше самой
    /// ставки превращает торги в один ход, а без потолка расчет шага не
    /// помещался в `numeric(14,2)` и уходил наружу пятисоткой
    #[schema(value_type = String, example = "7.5")]
    pub bid_step_percent: Decimal,
}

/// Открытие комнаты лота (FR-601): фиксирует стартовую ставку (INV-062)
/// и выбранный шаг от 5 % до 100 % (Q-019).
#[utoipa::path(
    post,
    path = "/api/v1/lots/{id}/auction",
    tag = "auctions",
    params(("id" = Uuid, Path, description = "Лот")),
    request_body = ScheduleAuctionRequest,
    responses(
        (status = 200, description = "Комната торгов", body = AuctionDto),
        (status = 404, description = "Лот не найден", body = crate::error::Problem),
        (status = 409, description = "Нет допущенных заявок с ценой", body = crate::error::Problem),
        (status = 422, description = "Шаг торгов вне диапазона 5-100 %", body = crate::error::Problem),
    )
)]
pub async fn schedule_auction(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(lot_id): Path<Uuid>,
    Json(body): Json<ScheduleAuctionRequest>,
) -> Result<Json<AuctionDto>, ApiError> {
    user.require(Action::AuctionManage)?;
    let step_percent = BidStepPercent::new(body.bid_step_percent)
        .map_err(|error| ApiError::Validation(error.to_string()))?;

    let record = auctions::schedule_for_lot(&state.db, user.id(), lot_id, step_percent)
        .await
        .map_err(|err| match err {
            ScheduleError::LotNotFound => ApiError::NotFound,
            ScheduleError::NoAdmittedBids => ApiError::rule(
                RuleViolation::AuctionStartPriceMissing,
                "стартовая ставка не определена: по лоту нет допущенных заявок с ценой (п. 62)",
            ),
            ScheduleError::Db(db) => db.into(),
        })?;

    let current_max = current_max(&state, record.id).await?;
    Ok(Json(AuctionDto::new(record, current_max)))
}

/// Комната лота, если она уже открыта, - точка входа из карточки тендера.
#[utoipa::path(
    get,
    path = "/api/v1/lots/{id}/auction",
    tag = "auctions",
    params(("id" = Uuid, Path, description = "Лот")),
    responses(
        (status = 200, description = "Комната торгов", body = AuctionDto),
        (status = 404, description = "Торги по лоту не открыты", body = crate::error::Problem),
    )
)]
pub async fn lot_auction(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(lot_id): Path<Uuid>,
) -> Result<Json<AuctionDto>, ApiError> {
    user.require(Action::AuctionWatch)?;

    let record = auctions::by_lot(&state.db, lot_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let current_max = current_max(&state, record.id).await?;
    Ok(Json(AuctionDto::new(record, current_max)))
}

/// Снимок комнаты (FR-603): состояние, лента, право зрителя торговаться.
#[utoipa::path(
    get,
    path = "/api/v1/auctions/{id}",
    tag = "auctions",
    params(("id" = Uuid, Path, description = "Торги")),
    responses(
        (status = 200, description = "Снимок комнаты", body = AuctionRoomDto),
        (status = 404, description = "Торги не найдены", body = crate::error::Problem),
    )
)]
pub async fn auction_room(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<AuctionRoomDto>, ApiError> {
    user.require(Action::AuctionWatch)?;

    let record = auctions::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let my_application_id = room_access(&state, &user, record.lot_id).await?;
    let bids = auctions::bids_of(&state.db, id, None).await?;
    let current_max = current_max(&state, id).await?;
    let participants = auction_turns::participants(&state.db, id).await?;

    Ok(Json(AuctionRoomDto {
        current_turn_application_id: record.current_turn_application_id,
        auction: AuctionDto::new(record, current_max),
        bids: bids.into_iter().map(BidDto::from).collect(),
        my_application_id,
        participants: participants
            .into_iter()
            .map(CircleParticipantDto::from)
            .collect(),
        server_time: now(&state).await?,
    }))
}

/// Объявление старта председателем (FR-602): таймер 60 минут (в демо-режиме -
/// длительность из параметра запуска api).
#[utoipa::path(
    post,
    path = "/api/v1/auctions/{id}/start",
    tag = "auctions",
    params(("id" = Uuid, Path, description = "Торги")),
    responses(
        (status = 200, description = "Торги идут", body = AuctionDto),
        (status = 404, description = "Торги не найдены", body = crate::error::Problem),
        (status = 409, description = "Старт из этого статуса невозможен", body = crate::error::Problem),
    )
)]
pub async fn start_auction(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<AuctionDto>, ApiError> {
    user.require(Action::AuctionManage)?;

    let record = auctions::start(&state.db, user.id(), id, state.auction_minutes)
        .await
        .map_err(map_transition)?;
    Ok(Json(broadcast_state(&state, record, None)))
}

/// Продление таймера решением председателя: ровно 15 минут, один раз (INV-066).
#[utoipa::path(
    post,
    path = "/api/v1/auctions/{id}/extend",
    tag = "auctions",
    params(("id" = Uuid, Path, description = "Торги")),
    responses(
        (status = 200, description = "Таймер продлен", body = AuctionDto),
        (status = 404, description = "Торги не найдены", body = crate::error::Problem),
        (status = 409, description = "Продление уже израсходовано (п. 68)", body = crate::error::Problem),
    )
)]
pub async fn extend_auction(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<AuctionDto>, ApiError> {
    user.require(Action::AuctionManage)?;

    let record = auctions::extend(&state.db, user.id(), id)
        .await
        .map_err(map_transition)?;
    let current_max = current_max(&state, record.id).await?;
    Ok(Json(broadcast_state(&state, record, current_max)))
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct FinishRequest {
    /// Досрочное завершение при общем согласии (п. 67)
    #[serde(default)]
    pub early: bool,
}

/// Завершение торгов (FR-606): победитель и второе место с их ставками.
#[utoipa::path(
    post,
    path = "/api/v1/auctions/{id}/finish",
    tag = "auctions",
    params(("id" = Uuid, Path, description = "Торги")),
    request_body = FinishRequest,
    responses(
        (status = 200, description = "Торги завершены", body = AuctionDto),
        (status = 404, description = "Торги не найдены", body = crate::error::Problem),
        (status = 409, description = "Завершать нечего", body = crate::error::Problem),
    )
)]
pub async fn finish_auction(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<FinishRequest>,
) -> Result<Json<AuctionDto>, ApiError> {
    user.require(Action::AuctionManage)?;

    let (record, _outcome) = auctions::finish(&state.db, user.id(), id, body.early)
        .await
        .map_err(map_transition)?;
    let current_max = current_max(&state, record.id).await?;
    Ok(Json(broadcast_state(&state, record, current_max)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PlaceBidRequest {
    /// uuid v7 ставки, сгенерированный клиентом: повтор после реконнекта
    /// возвращает ту же ставку (NFR-05)
    pub id: Uuid,
    #[schema(value_type = String, example = "57750")]
    pub amount: Decimal,
}

/// Ставка допущенного участника (FR-601). Ниже «максимум + шаг» отклоняется
/// (INV-063), после истечения таймера - тоже (INV-066).
#[utoipa::path(
    post,
    path = "/api/v1/auctions/{id}/bids",
    tag = "auctions",
    params(("id" = Uuid, Path, description = "Торги")),
    request_body = PlaceBidRequest,
    responses(
        (status = 201, description = "Ставка принята", body = BidDto),
        (status = 403, description = "Участник не допущен к этому лоту", body = crate::error::Problem),
        (status = 404, description = "Торги не найдены", body = crate::error::Problem),
        (status = 409, description = "Ставка отклонена правилами (INV-063/INV-066)", body = crate::error::Problem),
    )
)]
pub async fn place_bid(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<PlaceBidRequest>,
) -> Result<(StatusCode, Json<BidDto>), ApiError> {
    user.require(Action::BidPlace)?;

    let record = auctions::place_bid(
        &state.db,
        user.id(),
        NewBid {
            id: body.id,
            auction_id: id,
            amount: body.amount,
        },
    )
    .await
    .map_err(|err| match err {
        BidError::NotFound => ApiError::NotFound,
        BidError::NotAdmitted => ApiError::Forbidden,
        BidError::Rejected(reason) => ApiError::RuleViolation(reason),
        BidError::Db(db) => db.into(),
    })?;

    let bid = BidDto::from(record);
    // Лента комнаты обновляется у всех присутствующих (п. 65)
    if let Some(auction) = auctions::get(&state.db, id).await? {
        let dto = AuctionDto::new(auction, Some(bid.amount));
        state.auction_hub.publish(
            id,
            &RoomEvent::Bid {
                bid: bid.clone(),
                auction: dto,
            },
        );
    }
    // Ход перешел следующему по кругу (FR-604)
    publish_turn(&state, id).await?;

    Ok((StatusCode::CREATED, Json(bid)))
}

/// Пас участника (FR-604, п. 65): не готов повысить - выбывает из торгов.
/// Когда соперников не остается, торги завершаются автоматически: победитель
/// - единственный оставшийся с его последней ставкой (FR-606).
#[utoipa::path(
    post,
    path = "/api/v1/auctions/{id}/pass",
    tag = "auctions",
    params(("id" = Uuid, Path, description = "Торги")),
    responses(
        (status = 200, description = "Ход передан либо торги завершены", body = AuctionDto),
        (status = 403, description = "Участник не в круге этих торгов", body = crate::error::Problem),
        (status = 409, description = "Сейчас ход другого участника", body = crate::error::Problem),
    )
)]
pub async fn pass_turn(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<AuctionDto>, ApiError> {
    user.require(Action::BidPlace)?;

    let record = auctions::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let application_id = auctions::admitted_application_of(&state.db, user.id(), record.lot_id)
        .await?
        .ok_or(ApiError::Forbidden)?;

    let progress = auction_turns::pass(&state.db, user.id(), id, application_id)
        .await
        .map_err(circle_error)?;
    publish_turn(&state, id).await?;

    // Соперников не осталось - торги окончены (п. 65), а не «досрочно
    // по общему согласию» (п. 67): флаг досрочности не ставится
    if matches!(progress, Progress::Finished) {
        let (record, _outcome) = auctions::finish(&state.db, user.id(), id, false)
            .await
            .map_err(map_transition)?;
        let current_max = current_max(&state, id).await?;
        return Ok(Json(broadcast_state(&state, record, current_max)));
    }

    let current_max = current_max(&state, id).await?;
    Ok(Json(AuctionDto::new(record, current_max)))
}

/// Отметка неявки допущенного участника (FR-605, п. 70): его первоначальное
/// предложение оглашается в ленте, повышать он не может. При неявке всех
/// победитель определяется по максимальному первоначальному предложению -
/// секретарю остается завершить торги.
#[utoipa::path(
    post,
    path = "/api/v1/auctions/{id}/participants/{application_id}/absent",
    tag = "auctions",
    params(
        ("id" = Uuid, Path, description = "Торги"),
        ("application_id" = Uuid, Path, description = "Заявка участника"),
    ),
    responses(
        (status = 200, description = "Неявка отмечена, предложение оглашено", body = AuctionRoomDto),
        (status = 404, description = "Торги или участник не найдены", body = crate::error::Problem),
        (status = 409, description = "Отметить неявку сейчас нельзя", body = crate::error::Problem),
    )
)]
pub async fn mark_absent(
    user: CurrentUser,
    State(state): State<AppState>,
    Path((id, application_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<AuctionRoomDto>, ApiError> {
    user.require(Action::AuctionManage)?;

    auction_turns::mark_absent(&state.db, user.id(), id, application_id)
        .await
        .map_err(circle_error)?;
    publish_turn(&state, id).await?;

    // Неявка могла оставить круг без соперников: торги идут, пока не
    // останется один (п. 65), а при неявке всех победитель определяется
    // по максимальному первоначальному предложению без торгов (п. 71)
    let record = auctions::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if record.status == "running" && !auction_turns::rivals_remain(&state.db, id).await? {
        let (record, _outcome) = auctions::finish(&state.db, user.id(), id, false)
            .await
            .map_err(map_transition)?;
        let current_max = current_max(&state, id).await?;
        broadcast_state(&state, record, current_max);
    }

    room_snapshot(&state, &user, id).await.map(Json)
}

#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
pub struct BidsParams {
    /// Курсор реконнекта: вернуть ставки с номером больше указанного (FR-607)
    pub after_seq: Option<i64>,
}

/// Догрузка ленты после разрыва связи (FR-607, NFR-05): клиент называет
/// последний известный ему номер ставки и получает только пропущенное.
#[utoipa::path(
    get,
    path = "/api/v1/auctions/{id}/bids",
    tag = "auctions",
    params(("id" = Uuid, Path, description = "Торги"), BidsParams),
    responses(
        (status = 200, description = "Лента ставок", body = [BidDto]),
        (status = 403, description = "Участник не допущен к лоту этих торгов", body = crate::error::Problem),
        (status = 404, description = "Торги не найдены", body = crate::error::Problem),
    )
)]
pub async fn auction_bids(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    crate::request::Query(params): crate::request::Query<BidsParams>,
) -> Result<Json<Vec<BidDto>>, ApiError> {
    user.require(Action::AuctionWatch)?;

    // Догрузка отдает ту же ленту, что и снимок, - и закрыта тем же правилом
    let record = auctions::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    room_access(&state, &user, record.lot_id).await?;

    let bids = auctions::bids_of(&state.db, id, params.after_seq).await?;
    Ok(Json(bids.into_iter().map(BidDto::from).collect()))
}

fn circle_error(err: CircleError) -> ApiError {
    match err {
        CircleError::NotFound => ApiError::NotFound,
        CircleError::Rejected(reason) => ApiError::RuleViolation(reason),
        CircleError::Db(db) => db.into(),
    }
}

/// Смена хода и состава круга - событие ленты для всех присутствующих.
async fn publish_turn(state: &AppState, auction_id: Uuid) -> Result<(), ApiError> {
    let participants = auction_turns::participants(&state.db, auction_id).await?;
    let current = auctions::get(&state.db, auction_id)
        .await?
        .and_then(|record| record.current_turn_application_id);

    state.auction_hub.publish(
        auction_id,
        &RoomEvent::Turn {
            participants: participants
                .into_iter()
                .map(CircleParticipantDto::from)
                .collect(),
            current_turn_application_id: current,
        },
    );
    Ok(())
}

/// Снимок комнаты одним куском - общий для REST-снимка и ответов действий.
async fn room_snapshot(
    state: &AppState,
    user: &CurrentUser,
    id: Uuid,
) -> Result<AuctionRoomDto, ApiError> {
    let record = auctions::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let my_application_id = room_access(state, user, record.lot_id).await?;
    let bids = auctions::bids_of(&state.db, id, None).await?;
    let current_max = current_max(state, id).await?;
    let participants = auction_turns::participants(&state.db, id).await?;

    Ok(AuctionRoomDto {
        current_turn_application_id: record.current_turn_application_id,
        auction: AuctionDto::new(record, current_max),
        bids: bids.into_iter().map(BidDto::from).collect(),
        my_application_id,
        participants: participants
            .into_iter()
            .map(CircleParticipantDto::from)
            .collect(),
        server_time: now(state).await?,
    })
}

/// WS-комната (FR-603): после подключения - снимок состояния, дальше поток
/// событий. Маршрут вне OpenAPI: контракт кодогена описывает только HTTP.
///
/// Апгрейд сокета не проходит проверку источника, которую браузер делает за
/// обычный запрос: CORS на upgrade не распространяется. Поэтому `Origin`
/// сверяется здесь, а не слоем выше (см. [`AppState::ws_origin_allowed`]).
pub async fn room_socket(
    user: CurrentUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    user.require(Action::AuctionWatch)?;

    // Заголовок в не-UTF-8 - это не «заголовка нет»: сверить его не с чем,
    // и апгрейд отклоняется наравне с чужим origin
    let origin = headers
        .get(header::ORIGIN)
        .map(|value| value.to_str().unwrap_or_default());
    if !state.ws_origin_allowed(origin) {
        return Err(ApiError::Forbidden);
    }

    let record = auctions::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    room_access(&state, &user, record.lot_id).await?;
    let current_max = current_max(&state, id).await?;
    let snapshot = RoomEvent::State {
        auction: AuctionDto::new(record, current_max),
    };
    let events = state.auction_hub.subscribe(id);

    Ok(upgrade.on_upgrade(move |socket| feed(socket, snapshot, events)))
}

/// Комната только раздает события: входящие кадры игнорируются, ставки
/// принимает REST (единая точка правил и аудита).
async fn feed(
    mut socket: WebSocket,
    snapshot: RoomEvent,
    events: tokio::sync::broadcast::Receiver<std::sync::Arc<str>>,
) {
    if let Ok(json) = serde_json::to_string(&snapshot)
        && socket.send(Message::text(json)).await.is_err()
    {
        return;
    }

    let mut stream = BroadcastStream::new(events);
    while let Some(message) = stream.next().await {
        // Lagged (отставший клиент) - пропуск: состояние он дотянет снимком
        let Ok(json) = message else { continue };
        if socket.send(Message::text(json.as_ref())).await.is_err() {
            return;
        }
    }
}

fn broadcast_state(state: &AppState, record: AuctionRecord, max: Option<Decimal>) -> AuctionDto {
    let dto = AuctionDto::new(record, max);
    state.auction_hub.publish(
        dto.id,
        &RoomEvent::State {
            auction: dto.clone(),
        },
    );
    dto
}

/// Текущий максимум ленты (INV-063). Считает БД одним запросом: перебор
/// выборки ставок и стоил дороже с каждой минутой торгов (NFR-02), и врал
/// после `MAX_ROWS`-й ставки - подробности в [`auctions::max_amount`].
async fn current_max(state: &AppState, auction_id: Uuid) -> Result<Option<Decimal>, ApiError> {
    Ok(auctions::max_amount(&state.db, auction_id).await?)
}

/// Роль, ведущая процесс: любая, кроме участнической, которой политика
/// открывает комнату (организатор, секретарь, комиссия). Им лента нужна
/// по всем лотам - они ее и оглашают (п. 65).
fn leads_process(roles: &[Role]) -> bool {
    roles
        .iter()
        .any(|role| *role != Role::Participant && is_allowed(*role, Action::AuctionWatch))
}

/// Право видеть ленту комнаты. Участнику ее открывает допущенная заявка на
/// лот, а не сама роль: `AuctionWatch` есть у любого зарегистрированного
/// участника, и одного идентификатора торгов хватало, чтобы читать ставки
/// и имена заявителей чужого лота в реальном времени. Несколько ролей
/// доступа не отнимают - достаточно одной ведущей процесс.
fn room_access_granted(roles: &[Role], my_application_id: Option<Uuid>) -> bool {
    my_application_id.is_some() || leads_process(roles)
}

/// Проверка доступа к ленте и собственная заявка зрителя одним походом в БД:
/// снимку комнаты она нужна и так (`my_application_id`).
async fn room_access(
    state: &AppState,
    user: &CurrentUser,
    lot_id: Uuid,
) -> Result<Option<Uuid>, ApiError> {
    let my_application_id = auctions::admitted_application_of(&state.db, user.id(), lot_id).await?;
    if !room_access_granted(&user.roles, my_application_id) {
        return Err(ApiError::Forbidden);
    }
    Ok(my_application_id)
}

/// Часы сервера (NFR-03): берутся из БД - единственного источника времени.
async fn now(state: &AppState) -> Result<OffsetDateTime, ApiError> {
    let value = sqlx::query_scalar!(r#"SELECT core.now() AS "now!""#)
        .fetch_one(&state.db)
        .await?;
    Ok(value)
}

fn map_transition(error: TransitionError) -> ApiError {
    match error {
        TransitionError::NotFound => ApiError::NotFound,
        TransitionError::Rejected(reason) => ApiError::RuleViolation(reason),
        TransitionError::Db(db) => db.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_event_wire_shape_is_tagged() {
        let event = RoomEvent::State {
            auction: sample_auction(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "state");
        assert_eq!(json["auction"]["min_next_bid"], "57750");
    }

    /// INV-063 на проводе: минимум для клиента считается от максимума ленты.
    #[test]
    fn min_next_bid_follows_current_max() {
        let mut record = sample_record();
        record.starting_bid = "55000".parse().unwrap();
        let dto = AuctionDto::new(record, Some("57750".parse().unwrap()));
        assert_eq!(dto.min_next_bid, "60500".parse::<Decimal>().unwrap());
    }

    fn sample_record() -> AuctionRecord {
        AuctionRecord {
            id: Uuid::nil(),
            lot_id: Uuid::nil(),
            tender_id: Uuid::nil(),
            lot_seq: 1,
            lot_purpose: "офис".into(),
            status: "scheduled".into(),
            starting_bid: "55000".parse().unwrap(),
            bid_step_percent: "5".parse().unwrap(),
            bid_step: "2750".parse().unwrap(),
            started_at: None,
            ends_at: None,
            extended_once: false,
            finished_at: None,
            finished_early: false,
            winner_application_id: None,
            winner_amount: None,
            current_turn_application_id: None,
            runner_up_application_id: None,
            runner_up_amount: None,
        }
    }

    fn sample_auction() -> AuctionDto {
        AuctionDto::new(sample_record(), None)
    }

    /// Участник без допущенной заявки на этот лот в комнату не входит:
    /// иначе по одному идентификатору торгов он читал бы ставки и имена
    /// заявителей чужого лота.
    #[test]
    fn participant_without_admitted_application_is_denied() {
        assert!(!room_access_granted(&[Role::Participant], None));
        assert!(room_access_granted(&[Role::Participant], Some(Uuid::nil())));
    }

    /// Роли, ведущие процесс, оглашают ленту (п. 65) - им нужна любая комната.
    #[test]
    fn process_roles_watch_any_room() {
        for role in [Role::Organizer, Role::Secretary, Role::Commission] {
            assert!(room_access_granted(&[role], None), "{role:?} ведет процесс");
        }
    }

    /// Совмещение ролей не отнимает доступ: секретарь, у которого есть и
    /// участническая роль, остается ведущим процесс.
    #[test]
    fn extra_participant_role_does_not_narrow_access() {
        assert!(room_access_granted(
            &[Role::Participant, Role::Secretary],
            None
        ));
    }

    /// Роль без права смотреть комнату вовсе (финансы, правление, админ)
    /// ведущей процесс не считается - иначе проверка открыла бы ленту тем,
    /// кому политика ее не давала.
    #[test]
    fn roles_without_watch_right_do_not_lead_the_process() {
        for role in [Role::Finance, Role::Board, Role::Admin, Role::Guest] {
            assert!(!leads_process(&[role]), "{role:?} не ведет торги");
        }
    }
}
