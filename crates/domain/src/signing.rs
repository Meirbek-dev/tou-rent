//! Подписание документов (ТЗ § 2 - задел, ответ заказчика № 10).
//!
//! ЭЦП НУЦ РК вне периметра: юридически значимый экземпляр договора и акта -
//! печатная форма плюс скан подписанного документа. Модуль существует не для
//! того, чтобы что-то подписывать, а чтобы способ подписания был выражен
//! типом: подключение реальной подписи в контуре 5 добавляет реализацию
//! [`SigningProvider`] и вариант [`SignatureStatus::Electronic`], но не
//! переделывает документ.

use serde::{Deserialize, Serialize};

/// Способ подписания документа (паритет с enum БД `core.signature_status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignatureStatus {
    /// Подписанного экземпляра нет
    Unsigned,
    /// Подписан на бумаге - загружен скан (текущий периметр, п. 111–115)
    Paper,
    /// Подписан ЭЦП; ставит только провайдер подписи
    Electronic,
}

impl SignatureStatus {
    pub const ALL: [SignatureStatus; 3] = [
        SignatureStatus::Unsigned,
        SignatureStatus::Paper,
        SignatureStatus::Electronic,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SignatureStatus::Unsigned => "unsigned",
            SignatureStatus::Paper => "paper",
            SignatureStatus::Electronic => "electronic",
        }
    }

    /// Способ подписания - производное от факта, а не отдельно вводимое
    /// значение: появился скан - документ подписан на бумаге, скан снят -
    /// снова не подписан. Электронная подпись сильнее бумажной и снятием
    /// скана не отменяется.
    ///
    /// Правило продублировано триггером `core.sync_signature_status`;
    /// паритет проверяет testkit-тест.
    pub fn with_scan(self, has_scan: bool) -> Self {
        match self {
            SignatureStatus::Electronic => SignatureStatus::Electronic,
            _ if has_scan => SignatureStatus::Paper,
            _ => SignatureStatus::Unsigned,
        }
    }

    /// Документ считается подписанным любым способом.
    pub fn is_signed(self) -> bool {
        !matches!(self, SignatureStatus::Unsigned)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестный способ подписания: {0}")]
pub struct UnknownSignatureStatus(pub String);

impl std::str::FromStr for SignatureStatus {
    type Err = UnknownSignatureStatus;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "unsigned" => Ok(SignatureStatus::Unsigned),
            "paper" => Ok(SignatureStatus::Paper),
            "electronic" => Ok(SignatureStatus::Electronic),
            other => Err(UnknownSignatureStatus(other.to_owned())),
        }
    }
}

/// Документ, предъявляемый к подписи. Содержимое не передается: провайдер
/// подписывает отпечаток, поэтому файл не покидает хранилище.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentToSign<'a> {
    /// Вид документа: договор, акт, допсоглашение
    pub kind: &'a str,
    /// sha256 печатной формы - тот же отпечаток, что и в hash-цепочке аудита
    pub content_sha256: [u8; 32],
}

/// Результат подписания. Хранение и привязка к документу - дело адаптера.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub provider: &'static str,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SigningError {
    /// Провайдер подписи в периметре отсутствует - документ подписывается
    /// на бумаге (ТЗ § 2). Не ошибка данных: единственное состояние заглушки.
    #[error("подписание недоступно: {0}")]
    Unavailable(&'static str),
}

/// Провайдер подписи. Реализация в контуре 5 (ЭЦП НУЦ РК) не меняет
/// вызывающий код - меняется только значение [`SignatureStatus`].
pub trait SigningProvider {
    /// Имя провайдера для аудита и печатной формы.
    fn name(&self) -> &'static str;

    /// Статус, который получает документ после успешного подписания.
    fn resulting_status(&self) -> SignatureStatus;

    fn sign(&self, document: &DocumentToSign<'_>) -> Result<Signature, SigningError>;
}

/// Заглушка периметра: подписание идет на бумаге, электронной подписи нет.
/// Возвращает отказ всегда - это честное состояние, а не заготовка кода.
#[derive(Debug, Clone, Copy, Default)]
pub struct PaperSigning;

impl SigningProvider for PaperSigning {
    fn name(&self) -> &'static str {
        "paper"
    }

    fn resulting_status(&self) -> SignatureStatus {
        SignatureStatus::Paper
    }

    fn sign(&self, _document: &DocumentToSign<'_>) -> Result<Signature, SigningError> {
        Err(SigningError::Unavailable(
            "ЭЦП вне периметра (ТЗ § 2): экземпляр подписывается на бумаге, \
             подтверждение - загруженный скан",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_makes_document_paper_signed() {
        assert_eq!(
            SignatureStatus::Unsigned.with_scan(true),
            SignatureStatus::Paper
        );
        assert_eq!(
            SignatureStatus::Paper.with_scan(false),
            SignatureStatus::Unsigned,
            "снятый скан снимает и бумажную подпись"
        );
    }

    #[test]
    fn electronic_signature_survives_scan_changes() {
        for has_scan in [true, false] {
            assert_eq!(
                SignatureStatus::Electronic.with_scan(has_scan),
                SignatureStatus::Electronic,
                "электронная подпись сильнее бумажной"
            );
        }
    }

    #[test]
    fn status_round_trips_through_db_representation() {
        for status in SignatureStatus::ALL {
            assert_eq!(status.as_str().parse(), Ok(status));
        }
        assert!("qualified".parse::<SignatureStatus>().is_err());
    }

    #[test]
    fn only_unsigned_counts_as_not_signed() {
        assert!(!SignatureStatus::Unsigned.is_signed());
        assert!(SignatureStatus::Paper.is_signed());
        assert!(SignatureStatus::Electronic.is_signed());
    }

    /// Заглушка не притворяется подписывающей: подпись невозможна, и это
    /// видно из типа результата, а не из комментария.
    #[test]
    fn paper_provider_never_signs() {
        let document = DocumentToSign {
            kind: "contract",
            content_sha256: [0; 32],
        };
        let error = PaperSigning
            .sign(&document)
            .expect_err("подписание недоступно");
        assert!(matches!(error, SigningError::Unavailable(_)));
        assert_eq!(PaperSigning.resulting_status(), SignatureStatus::Paper);
    }
}
