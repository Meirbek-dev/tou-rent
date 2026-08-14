//! Общие enum-DTO контракта. Значения зеркалят enum-типы БД (snake_case);
//! рассинхрон ловится тестами паритета и громкой ошибкой `from_db`.

use serde::{Deserialize, Serialize};
use tou_domain::tender::TenderStatus;
use utoipa::ToSchema;

use crate::error::ApiError;

/// Календарные даты на проводе - строки `YYYY-MM-DD`.
///
/// У `time::Date` собственного человекочитаемого serde-формата нет: без этого
/// модуля дата ждет числовую форму и «2026-08-01» не разбирается. Формат
/// задается один раз и используется всеми DTO с датами.
pub mod iso_date {
    time::serde::format_description!(
        inner,
        Date,
        time::macros::format_description!("[year]-[month]-[day]")
    );

    pub use inner::{deserialize, serialize};

    pub mod option {
        pub use super::inner::option::{deserialize, serialize};
    }
}

/// Курсор реестра на проводе (ТЗ § 7): непрозрачная для клиента строка
/// `<наносекунды unix>~<uuid>`.
///
/// Непрозрачная - значит, клиент ее не собирает и не разбирает, а возвращает
/// в `after` как есть: состав ключа принадлежит серверу и меняется вместе
/// с сортировкой реестра. Числом, а не RFC 3339: в строке запроса `+`
/// часового пояса читается как пробел, и такой курсор ломался бы ровно
/// на тех реестрах, где время записано с положительным смещением.
pub mod cursor {
    use tou_db::RowCursor;
    use uuid::Uuid;

    use crate::error::ApiError;

    pub fn encode(cursor: RowCursor) -> String {
        format!("{}~{}", cursor.at.unix_timestamp_nanos(), cursor.id)
    }

    pub fn parse(raw: &str) -> Result<RowCursor, ApiError> {
        let invalid = || ApiError::Validation(format!("курсор «{raw}» не разбирается"));

        let (nanos, id) = raw.split_once('~').ok_or_else(invalid)?;
        let at = nanos
            .parse::<i128>()
            .ok()
            .and_then(|nanos| time::OffsetDateTime::from_unix_timestamp_nanos(nanos).ok())
            .ok_or_else(invalid)?;
        let id: Uuid = id.parse().map_err(|_| invalid())?;
        Ok(RowCursor::new(at, id))
    }

    /// Курсор следующей страницы: он есть, только если за отданными строками
    /// что-то осталось, - иначе клиент запрашивал бы заведомо пустую страницу.
    pub fn next(truncated: bool, last: Option<RowCursor>) -> Option<String> {
        truncated.then(|| last.map(encode)).flatten()
    }
}

/// Десериализация значения БД в DTO-enum по serde-имени; неизвестное
/// значение - дрейф контракта, отвечаем 500 и шумим в телеметрию.
fn from_db_str<T: serde::de::DeserializeOwned>(kind: &str, raw: &str) -> Result<T, ApiError> {
    serde_json::from_value(serde_json::Value::String(raw.to_owned())).map_err(|_| {
        ApiError::internal(std::io::Error::other(format!(
            "{kind}: неизвестное значение БД '{raw}' - рассинхрон enum'ов"
        )))
    })
}

/// Тип объекта имущества (FR-101, БД `core.object_kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKindDto {
    Building,
    Premises,
    Structure,
    LandPlot,
}

impl ObjectKindDto {
    pub fn as_db(self) -> &'static str {
        match self {
            ObjectKindDto::Building => "building",
            ObjectKindDto::Premises => "premises",
            ObjectKindDto::Structure => "structure",
            ObjectKindDto::LandPlot => "land_plot",
        }
    }

    pub fn from_db(raw: &str) -> Result<Self, ApiError> {
        from_db_str("object_kind", raw)
    }
}

/// Вычисляемый статус объекта (FR-103, view `core.object_statuses`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObjectStatusDto {
    Free,
    InTender,
    Leased,
}

impl ObjectStatusDto {
    pub fn as_db(self) -> &'static str {
        match self {
            ObjectStatusDto::Free => "free",
            ObjectStatusDto::InTender => "in_tender",
            ObjectStatusDto::Leased => "leased",
        }
    }

    pub fn from_db(raw: &str) -> Result<Self, ApiError> {
        from_db_str("object_status", raw)
    }
}

/// Статус заявки (М4, БД `core.application_status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationStatusDto {
    Submitted,
    Withdrawn,
    FeeConfirmed,
    Admitted,
    Rejected,
}

impl ApplicationStatusDto {
    pub fn from_db(raw: &str) -> Result<Self, ApiError> {
        from_db_str("application_status", raw)
    }
}

/// Вид заявителя (Прил. 2, БД `core.applicant_kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApplicantKindDto {
    Individual,
    LegalEntity,
}

impl ApplicantKindDto {
    pub fn as_db(self) -> &'static str {
        match self {
            ApplicantKindDto::Individual => "individual",
            ApplicantKindDto::LegalEntity => "legal_entity",
        }
    }

    pub fn from_db(raw: &str) -> Result<Self, ApiError> {
        from_db_str("applicant_kind", raw)
    }
}

/// Вид записи журнала регистрации (Прил. 12, БД `core.journal_entry_kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum JournalEntryKindDto {
    ApplicationSubmitted,
    ApplicationWithdrawn,
}

impl JournalEntryKindDto {
    pub fn from_db(raw: &str) -> Result<Self, ApiError> {
        from_db_str("journal_entry_kind", raw)
    }
}

/// Статус тендера (FR-302) - контрактное зеркало `domain::tender::TenderStatus`;
/// паритет закреплен тестом ниже.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TenderStatusDto {
    Draft,
    Announced,
    Accepting,
    Qualification,
    Trading,
    SummedUp,
    Contracted,
    Failed,
    RepeatAnnounced,
    Cancelled,
}

impl TenderStatusDto {
    pub fn from_db(raw: &str) -> Result<Self, ApiError> {
        from_db_str("tender_status", raw)
    }
}

impl From<TenderStatus> for TenderStatusDto {
    fn from(status: TenderStatus) -> Self {
        match status {
            TenderStatus::Draft => TenderStatusDto::Draft,
            TenderStatus::Announced => TenderStatusDto::Announced,
            TenderStatus::Accepting => TenderStatusDto::Accepting,
            TenderStatus::Qualification => TenderStatusDto::Qualification,
            TenderStatus::Trading => TenderStatusDto::Trading,
            TenderStatus::SummedUp => TenderStatusDto::SummedUp,
            TenderStatus::Contracted => TenderStatusDto::Contracted,
            TenderStatus::Failed => TenderStatusDto::Failed,
            TenderStatus::RepeatAnnounced => TenderStatusDto::RepeatAnnounced,
            TenderStatus::Cancelled => TenderStatusDto::Cancelled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tender_status_dto_matches_domain_wire_format() {
        // Паритет serde-имен DTO с domain::TenderStatus::as_str (= enum БД)
        for status in TenderStatus::ALL {
            let dto = TenderStatusDto::from(status);
            let dto_wire = serde_json::to_value(dto).unwrap();
            assert_eq!(
                dto_wire,
                serde_json::Value::String(status.as_str().to_owned())
            );
            assert_eq!(TenderStatusDto::from_db(status.as_str()).unwrap(), dto);
        }
    }

    #[test]
    fn unknown_db_value_is_loud() {
        assert!(TenderStatusDto::from_db("nonsense").is_err());
        assert!(ObjectKindDto::from_db("hangar").is_err());
    }

    #[test]
    fn object_kind_roundtrip() {
        for kind in [
            ObjectKindDto::Building,
            ObjectKindDto::Premises,
            ObjectKindDto::Structure,
            ObjectKindDto::LandPlot,
        ] {
            assert_eq!(ObjectKindDto::from_db(kind.as_db()).unwrap(), kind);
        }
    }

    /// Курсор переживает дорогу до клиента и обратно без потери точности:
    /// момент события хранится в наносекундах, и округление до секунды
    /// увело бы страницу мимо строк той же секунды.
    #[test]
    fn cursor_survives_a_round_trip() {
        let at = time::OffsetDateTime::from_unix_timestamp_nanos(1_777_000_000_123_456_789)
            .expect("момент");
        let source = tou_db::RowCursor::new(at, uuid::Uuid::from_u128(42));

        let encoded = cursor::encode(source);
        assert_eq!(cursor::parse(&encoded).unwrap(), source);
        assert!(
            !encoded.contains('+') && !encoded.contains(' '),
            "курсор едет в строке запроса: {encoded}"
        );
    }

    /// Испорченный курсор - ошибка запроса, а не молчаливая первая страница:
    /// иначе клиент листал бы реестр по кругу, не понимая почему.
    #[test]
    fn a_broken_cursor_is_rejected() {
        assert!(cursor::parse("").is_err());
        assert!(cursor::parse("не-курсор").is_err());
        assert!(cursor::parse("123~не-uuid").is_err());
        assert!(cursor::parse("~").is_err());
    }

    /// Курсор следующей страницы есть только тогда, когда есть следующая
    /// страница: иначе клиент запросил бы заведомо пустую выдачу.
    #[test]
    fn the_next_cursor_appears_only_with_more_rows() {
        let cursor = tou_db::RowCursor::new(time::OffsetDateTime::UNIX_EPOCH, uuid::Uuid::nil());
        assert!(cursor::next(false, Some(cursor)).is_none());
        assert!(cursor::next(true, None).is_none());
        assert!(cursor::next(true, Some(cursor)).is_some());
    }

    /// Витрина FR-102 фильтрует по статусу строкой БД: рассинхрон
    /// `as_db`/`from_db` молча отдал бы пустую выдачу.
    #[test]
    fn object_status_roundtrip() {
        for status in [
            ObjectStatusDto::Free,
            ObjectStatusDto::InTender,
            ObjectStatusDto::Leased,
        ] {
            assert_eq!(ObjectStatusDto::from_db(status.as_db()).unwrap(), status);
        }
    }
}
