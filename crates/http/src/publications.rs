//! Публикация протоколов, копии участникам и досье - тендера и решения
//! особого порядка (М7, М12, М14, М16: FR-702, FR-703, FR-1206, FR-1402,
//! FR-1602, INV-042, INV-076).
//!
//! Публикация открывает протокол публично на шесть месяцев (срок считает БД)
//! и рассылает участникам копии (п. 56, 75). Досье собирается триггерами БД,
//! здесь - его состав и выгрузка архивом: материалы складываются в папки
//! по видам, а состав описывается манифестом. У досье два предмета - тендер
//! и заявка особого порядка (FR-1206): сборка и архив у них общие, различаются
//! права доступа и срок хранения материалов (INV-042).

use std::io::{Cursor, Write as _};

use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use object_store::ObjectStoreExt as _;
use object_store::path::Path as ObjectPath;
use serde::Serialize;
use serde_json::json;
use time::OffsetDateTime;
use tou_db::publications::{self, ProtocolRecord, PublicationError};
use tou_db::tenders;
use tou_domain::notification::NotificationKind;
use tou_domain::policy::{Action, Compound};
use tou_domain::publication::{DossierKind, DossierSubject};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::request::{Json, Path};
use crate::state::AppState;

fn publication_error(err: PublicationError) -> ApiError {
    match err {
        PublicationError::NotFound => ApiError::NotFound,
        PublicationError::Rejected(reason) => ApiError::RuleViolation(reason),
        PublicationError::Db(db) => db.into(),
    }
}

/// Протокол комиссии и состояние его публикации (FR-702, INV-076).
#[derive(Debug, Serialize, ToSchema)]
pub struct ProtocolDto {
    pub id: Uuid,
    pub tender_id: Uuid,
    pub tender_title: String,
    /// `admission` | `results` | `failed` | `winner2`
    pub kind: String,
    pub number: Option<String>,
    pub has_pdf: bool,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub generated_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub published_at: Option<OffsetDateTime>,
    /// Момент автоматического снятия: публикация + 6 месяцев (п. 76)
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub unpublish_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub unpublished_at: Option<OffsetDateTime>,
    /// Виден ли протокол публично сейчас (FR-1402)
    pub is_public: bool,
}

fn protocol_dto(record: ProtocolRecord) -> ProtocolDto {
    let is_public = record.facts().is_public();
    ProtocolDto {
        id: record.id,
        tender_id: record.tender_id,
        tender_title: record.tender_title,
        kind: record.kind,
        number: record.number,
        has_pdf: record.pdf_key.is_some(),
        generated_at: record.generated_at,
        published_at: record.published_at,
        unpublish_at: record.unpublish_at,
        unpublished_at: record.unpublished_at,
        is_public,
    }
}

/// Протоколы тендера. Гость и посторонний видят опубликованные (FR-1402),
/// участник тендера - все по своему тендеру (п. 56), комиссия и секретарь -
/// все.
#[utoipa::path(
    get,
    path = "/api/v1/tenders/{id}/protocols",
    tag = "publications",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses((status = 200, description = "Протоколы тендера", body = [ProtocolDto]))
)]
pub async fn tender_protocols(
    user: Option<CurrentUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<ProtocolDto>>, ApiError> {
    let records = publications::list_for_tender(&state.db, id).await?;
    let sees_all = sees_all_protocols(&state, user.as_ref(), id).await?;

    Ok(Json(
        records
            .into_iter()
            .filter(|record| sees_all || record.facts().is_public())
            .map(protocol_dto)
            .collect(),
    ))
}

/// Протоколы кабинета участника (ТЗ § 7).
///
/// `next_after` всегда `null`: выборку ограничивает участие одного человека,
/// продолжения у нее нет. Поле остается, чтобы усекаемые реестры отвечали
/// одинаково и клиенту не приходилось помнить, у какого из них какая форма.
#[derive(Debug, Serialize, ToSchema)]
pub struct MyProtocolPage {
    pub items: Vec<ProtocolDto>,
    pub next_after: Option<String>,
    /// Показана не вся выборка
    pub truncated: bool,
}

/// Копии протоколов в кабинете участника (FR-703, п. 56): по всем тендерам,
/// где участник подавал заявку, независимо от публичного срока.
#[utoipa::path(
    get,
    path = "/api/v1/protocols/my",
    tag = "publications",
    responses((status = 200, description = "Протоколы моих тендеров", body = MyProtocolPage))
)]
pub async fn my_protocols(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<MyProtocolPage>, ApiError> {
    user.require(Action::ApplicationReadOwn)?;

    let page = publications::list_for_participant(&state.db, user.id()).await?;
    let truncated = page.truncated;
    Ok(Json(MyProtocolPage {
        items: page.into_iter().map(protocol_dto).collect(),
        next_after: None,
        truncated,
    }))
}

/// Публикация протокола (FR-702, п. 75): публичный доступ на шесть месяцев
/// и копии участникам.
#[utoipa::path(
    post,
    path = "/api/v1/protocols/{id}/publish",
    tag = "publications",
    params(("id" = Uuid, Path, description = "Протокол")),
    responses(
        (status = 200, description = "Протокол опубликован", body = ProtocolDto),
        (status = 404, description = "Протокол не найден", body = crate::error::Problem),
        (status = 409, description = "Публикация невозможна (п. 75–76)", body = crate::error::Problem),
    )
)]
pub async fn publish_protocol(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ProtocolDto>, ApiError> {
    user.require(Action::ProtocolGenerate)?;

    let record = publications::publish(&state.db, user.id(), id)
        .await
        .map_err(publication_error)?;

    // Копия протокола участникам тендера (FR-703, п. 56)
    let participants = publications::participants_of(&state.db, record.tender_id).await?;
    let notices: Vec<tou_db::notifications::NewNotification> = participants
        .into_iter()
        .map(|participant_id| tou_db::notifications::NewNotification {
            user_id: participant_id,
            payload: json!({
                "tender_id": record.tender_id,
                "tender_title": record.tender_title,
                "protocol_id": record.id,
                "protocol_kind": record.kind,
                "protocol_number": record.number,
                "rule_ref": "п. 56, 75",
            }),
        })
        .collect();
    if !notices.is_empty() {
        tou_db::notifications::insert(
            &state.db,
            user.id(),
            NotificationKind::ProtocolPublished.as_str(),
            &notices,
        )
        .await?;
    }

    Ok(Json(protocol_dto(record)))
}

/// PDF протокола (FR-1402, FR-703): опубликованный доступен всем, снятый
/// и неопубликованный - участникам тендера, комиссии и секретарю.
#[utoipa::path(
    get,
    path = "/api/v1/protocols/{id}/pdf",
    tag = "publications",
    params(("id" = Uuid, Path, description = "Протокол")),
    responses(
        (status = 200, description = "PDF протокола", content_type = "application/pdf"),
        (status = 404, description = "Протокол или форма не найдены", body = crate::error::Problem),
    )
)]
pub async fn protocol_pdf(
    user: Option<CurrentUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let record = publications::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;

    if !record.facts().is_public()
        && !sees_all_protocols(&state, user.as_ref(), record.tender_id).await?
    {
        return Err(ApiError::NotFound);
    }

    let pdf_key = record.pdf_key.ok_or(ApiError::NotFound)?;
    let object = state
        .storage
        .get(&ObjectPath::from(pdf_key.as_str()))
        .await
        .map_err(ApiError::internal)?;
    let bytes = object.bytes().await.map_err(ApiError::internal)?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/pdf".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("inline; filename=\"protocol-{id}.pdf\""),
            ),
        ],
        bytes,
    )
        .into_response())
}

/// Видит ли пользователь непубличные протоколы тендера: участник этого
/// тендера (п. 56), комиссия и секретарь (FR-503).
async fn sees_all_protocols(
    state: &AppState,
    user: Option<&CurrentUser>,
    tender_id: Uuid,
) -> Result<bool, ApiError> {
    let Some(user) = user else { return Ok(false) };
    if user.require(Action::ApplicationReadAll).is_ok() {
        return Ok(true);
    }

    let participates = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM core.applications a
                        WHERE a.tender_id = $1 AND a.participant_id = $2) AS "participates!""#,
        tender_id,
        user.id()
    )
    .fetch_one(&state.db)
    .await?;
    Ok(participates)
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DossierItemDto {
    pub id: Uuid,
    /// Вид материала (`announcement`, `application`, `review`, `decision`, …)
    pub kind: String,
    pub kind_title_ru: String,
    pub title: Option<String>,
    pub has_file: bool,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub occurred_at: OffsetDateTime,
    /// Срок хранения материала (INV-042): 5 лет тендерные, 3 года решения
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub retain_until: OffsetDateTime,
}

fn dossier_dto(item: tou_db::publications::DossierItem) -> DossierItemDto {
    DossierItemDto {
        id: item.id,
        kind: item.kind.as_str().to_owned(),
        kind_title_ru: item.kind.title_ru().to_owned(),
        title: item.title,
        has_file: item.file_key.is_some(),
        occurred_at: item.occurred_at,
        retain_until: item.retain_until,
    }
}

/// Досье ведут те же, кто ведет процесс: организатор и секретарь комиссии
/// (`Compound::TENDER_PROCESS_READ`).
fn require_dossier_access(user: &CurrentUser) -> Result<(), ApiError> {
    user.require_any(Compound::TENDER_PROCESS_READ)
}

/// Досье тендера (FR-1602, п. 16): состав собирается автоматически.
#[utoipa::path(
    get,
    path = "/api/v1/tenders/{id}/dossier",
    tag = "publications",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses((status = 200, description = "Материалы досье", body = [DossierItemDto]))
)]
pub async fn tender_dossier(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<DossierItemDto>>, ApiError> {
    require_dossier_access(&user)?;

    let items = publications::dossier(&state.db, id).await?;
    Ok(Json(items.into_iter().map(dossier_dto).collect()))
}

/// Досье решения особого порядка (FR-1206, п. 97): заявка, ее документы,
/// заключение подразделения и решение Правления. Собирается тем же
/// механизмом, что и досье тендера, и доступно тем, кто ведет рассмотрение
/// (подразделение и Правление, A-075).
#[utoipa::path(
    get,
    path = "/api/v1/special-requests/{id}/dossier",
    tag = "publications",
    params(("id" = Uuid, Path, description = "Заявка особого порядка")),
    responses(
        (status = 200, description = "Материалы досье решения", body = [DossierItemDto]),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
    )
)]
pub async fn special_dossier(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<DossierItemDto>>, ApiError> {
    require_special_dossier_access(&user)?;

    let items = publications::special_dossier(&state.db, id).await?;
    Ok(Json(items.into_iter().map(dossier_dto).collect()))
}

/// Досье решения ведут те, кто его готовит: подразделение (п. 89) и
/// Правление (п. 90) - `Compound::SPECIAL_DECISION_ACCESS`.
fn require_special_dossier_access(user: &CurrentUser) -> Result<(), ApiError> {
    user.require_any(Compound::SPECIAL_DECISION_ACCESS)
}

/// Выгрузка досье тендера архивом (FR-1602): файлы по папкам видов
/// и манифест со всеми материалами - включая те, у которых своего файла нет.
#[utoipa::path(
    get,
    path = "/api/v1/tenders/{id}/dossier.zip",
    tag = "publications",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 200, description = "Архив досье", content_type = "application/zip"),
        (status = 404, description = "Тендер не найден", body = crate::error::Problem),
    )
)]
pub async fn dossier_archive(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    require_dossier_access(&user)?;

    let tender = tenders::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let items = publications::dossier(&state.db, id).await?;

    archive_response(
        &state,
        items,
        json!({
            "tender_id": id,
            "tender_title": tender.title,
            "subject": DossierSubject::Tender.as_str(),
            "retention_years": DossierSubject::Tender.retention_years(),
            "rule_ref": "п. 16, 42, FR-1602",
        }),
        format!("tender-{id}-dossier.zip"),
    )
    .await
}

/// Выгрузка досье решения архивом (FR-1206, п. 97): та же сборка, что
/// и у тендера, - разделы, манифест и срок хранения материалов.
#[utoipa::path(
    get,
    path = "/api/v1/special-requests/{id}/dossier.zip",
    tag = "publications",
    params(("id" = Uuid, Path, description = "Заявка особого порядка")),
    responses(
        (status = 200, description = "Архив досье решения", content_type = "application/zip"),
        (status = 404, description = "Заявка не найдена", body = crate::error::Problem),
    )
)]
pub async fn special_dossier_archive(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    require_special_dossier_access(&user)?;

    let request = tou_db::special::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let items = publications::special_dossier(&state.db, id).await?;

    archive_response(
        &state,
        items,
        json!({
            "special_request_id": id,
            "category": format!("{} ({})", request.category_label, request.category_rule_ref),
            "subject": DossierSubject::SpecialRequest.as_str(),
            "retention_years": DossierSubject::SpecialRequest.retention_years(),
            "rule_ref": "п. 97, 16.15, FR-1206",
        }),
        format!("special-request-{id}-dossier.zip"),
    )
    .await
}

/// Архив досье: манифест с составом (включая материалы без файла) и сами
/// файлы, разложенные по папкам разделов. `head` описывает предмет досье.
async fn archive_response(
    state: &AppState,
    items: Vec<tou_db::publications::DossierItem>,
    head: serde_json::Value,
    filename: String,
) -> Result<Response, ApiError> {
    let mut manifest = Vec::with_capacity(items.len());
    let mut files = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let entry = item.file_key.as_ref().map(|key| {
            let extension = key.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("bin");
            format!(
                "{}/{:02}-{}.{extension}",
                item.kind.folder(),
                index + 1,
                item.source_id.unwrap_or(item.id).simple()
            )
        });
        manifest.push(json!({
            "kind": item.kind.as_str(),
            "section": item.kind.title_ru(),
            "title": item.title,
            "occurred_at": item.occurred_at.unix_timestamp(),
            "retain_until": item.retain_until.unix_timestamp(),
            "source": item.source_table,
            "file": entry,
        }));
        if let (Some(key), Some(entry)) = (item.file_key.as_ref(), entry) {
            files.push((key.clone(), entry));
        }
    }

    // Отметка «сформировано» - по часам сервера (`core.now()`, ADR-0005)
    let generated_at = tou_db::refdata::now(&state.db).await?;

    let mut head = head;
    if let Some(object) = head.as_object_mut() {
        object.insert(
            "generated_at".to_owned(),
            json!(generated_at.unix_timestamp()),
        );
        object.insert("items".to_owned(), json!(manifest));
    }
    let manifest = serde_json::to_vec_pretty(&head).map_err(ApiError::internal)?;

    // Содержимое файлов достается из RustFS до сборки архива: zip пишет
    // синхронно, и держать в нем await'ы незачем
    let mut payloads = Vec::with_capacity(files.len());
    for (key, entry) in files {
        let object = state
            .storage
            .get(&ObjectPath::from(key.as_str()))
            .await
            .map_err(ApiError::internal)?;
        let bytes = object.bytes().await.map_err(ApiError::internal)?;
        payloads.push((entry, bytes.to_vec()));
    }

    let archive = tokio::task::spawn_blocking(move || build_archive(&manifest, &payloads))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        archive,
    )
        .into_response())
}

/// Сборка архива досье: манифест в корне, файлы - по папкам видов.
fn build_archive(manifest: &[u8], files: &[(String, Vec<u8>)]) -> std::io::Result<Vec<u8>> {
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer.start_file("manifest.json", options)?;
    writer.write_all(manifest)?;

    for (entry, bytes) in files {
        writer.start_file(entry.as_str(), options)?;
        writer.write_all(bytes)?;
    }

    Ok(writer.finish()?.into_inner())
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DossierSectionDto {
    pub kind: String,
    pub title_ru: String,
}

/// Разделы досье (FR-1602) - закрытый перечень видов материалов.
#[utoipa::path(
    get,
    path = "/api/v1/dossier-sections",
    tag = "publications",
    responses((status = 200, description = "Разделы досье", body = [DossierSectionDto]))
)]
pub async fn dossier_sections(user: CurrentUser) -> Result<Json<Vec<DossierSectionDto>>, ApiError> {
    user.require(Action::TenderRead)?;

    Ok(Json(
        DossierKind::ALL
            .into_iter()
            .map(|kind| DossierSectionDto {
                kind: kind.as_str().to_owned(),
                title_ru: kind.title_ru().to_owned(),
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Архив собирается с манифестом и файлами по папкам видов (FR-1602).
    #[test]
    fn dossier_archive_contains_manifest_and_files() {
        let manifest = br#"{"items":[]}"#;
        let files = vec![(
            "03-protocols/01-abc.pdf".to_owned(),
            b"%PDF-1.7 test".to_vec(),
        )];

        let archive = build_archive(manifest, &files).expect("архив");
        let mut zip = zip::ZipArchive::new(Cursor::new(archive)).expect("чтение архива");

        let names: Vec<String> = zip.file_names().map(str::to_owned).collect();
        assert!(names.contains(&"manifest.json".to_owned()));
        assert!(names.contains(&"03-protocols/01-abc.pdf".to_owned()));

        let mut entry = zip.by_name("03-protocols/01-abc.pdf").expect("файл");
        let mut content = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut content).expect("чтение файла");
        assert!(content.starts_with(b"%PDF"));
    }
}
