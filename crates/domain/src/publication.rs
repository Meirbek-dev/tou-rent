//! Публикация протоколов и досье (М7, М14, М16: FR-702, FR-703, FR-1206,
//! FR-1402, FR-1602, INV-042, INV-076, п. 6, 16, 42, 56, 75–76, 97).
//!
//! Публичность протокола - состояние со сроком, а не флаг: протокол
//! публикуется в течение двух рабочих дней (срок п. 75 живет в
//! [`crate::obligation`]), доступен шесть месяцев и снимается автоматически
//! (INV-076). Снятый протокол не исчезает - он остается в досье тендера
//! и в кабинете участника (п. 56): «снять с публикации» и «удалить» -
//! разные вещи, и вторая невозможна.
//!
//! Досье ведется по двум предметам ([`DossierSubject`]): тендер и заявка
//! особого порядка. Механизм у них один - материал попадает в досье в момент
//! события, - а различаются они сроком хранения: тендерные материалы лежат
//! не менее пяти лет, решения особого порядка - не менее трех (INV-042).

use serde::{Deserialize, Serialize};

/// Публичный доступ к протоколу - шесть месяцев с публикации (п. 76).
pub const PUBLIC_ACCESS_MONTHS: i32 = 6;

/// Состояние публикации протокола.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PublicationFacts {
    /// Печатная форма сформирована: публикуется документ, а не запись
    pub has_pdf: bool,
    pub published: bool,
    /// Срок публичного доступа истек, протокол снят джобом (INV-076)
    pub unpublished: bool,
}

impl PublicationFacts {
    /// Можно ли опубликовать протокол сейчас (FR-702).
    pub fn check_publish(&self) -> Result<(), PublicationError> {
        if !self.has_pdf {
            return Err(PublicationError::NoDocument);
        }
        if self.unpublished {
            return Err(PublicationError::AccessExpired);
        }
        if self.published {
            return Err(PublicationError::AlreadyPublished);
        }
        Ok(())
    }

    /// Виден ли протокол публично (портал, FR-1402).
    pub fn is_public(&self) -> bool {
        self.published && !self.unpublished
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PublicationError {
    #[error("печатная форма протокола не сформирована - публиковать нечего (п. 75)")]
    NoDocument,
    #[error("протокол уже опубликован (п. 75)")]
    AlreadyPublished,
    #[error(
        "срок публичного доступа ({PUBLIC_ACCESS_MONTHS} месяцев) истек, протокол снят \
         и хранится в досье (п. 76)"
    )]
    AccessExpired,
}

/// Вид материала досье (FR-1602, п. 16): досье собирается само из событий
/// процесса, поэтому перечень видов закрыт - новый материал добавляется
/// вместе с событием, которое его порождает.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DossierKind {
    /// Объявление о тендере и его редакции (Прил. 1, FR-303, FR-304)
    Announcement,
    /// Заявка участника с приложениями (Прил. 2, 9, 11) либо заявка
    /// особого порядка с документами категории (Прил. 3, п. 88)
    Application,
    /// Протокол комиссии (допуск, итоги, несостоявшийся, победитель № 2)
    Protocol,
    /// Факт публикации и снятия протокола (п. 75–76, INV-076)
    Publication,
    /// Заключение уполномоченного подразделения (FR-1202, п. 89)
    Review,
    /// Решение Правления по заявке особого порядка (FR-1202, п. 90)
    Decision,
    /// Договор с нанимателем (Прил. 5–6)
    Contract,
    /// Допсоглашение к договору (FR-906, п. 125)
    Amendment,
    /// Акт приема-передачи или возврата (Прил. 7–8)
    Act,
    /// Уклонение от подписания договора (п. 116)
    Evasion,
}

impl DossierKind {
    pub const ALL: [DossierKind; 10] = [
        DossierKind::Announcement,
        DossierKind::Application,
        DossierKind::Protocol,
        DossierKind::Publication,
        DossierKind::Review,
        DossierKind::Decision,
        DossierKind::Contract,
        DossierKind::Amendment,
        DossierKind::Act,
        DossierKind::Evasion,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            DossierKind::Announcement => "announcement",
            DossierKind::Application => "application",
            DossierKind::Protocol => "protocol",
            DossierKind::Publication => "publication",
            DossierKind::Review => "review",
            DossierKind::Decision => "decision",
            DossierKind::Contract => "contract",
            DossierKind::Amendment => "amendment",
            DossierKind::Act => "act",
            DossierKind::Evasion => "evasion",
        }
    }

    /// Название раздела досье (ru - делопроизводство, NFR-01).
    pub fn title_ru(self) -> &'static str {
        match self {
            DossierKind::Announcement => "объявление и редакции документации",
            DossierKind::Application => "заявки участников",
            DossierKind::Protocol => "протоколы комиссии",
            DossierKind::Publication => "публикация протоколов",
            DossierKind::Review => "заключения подразделения",
            DossierKind::Decision => "решения Правления",
            DossierKind::Contract => "договоры",
            DossierKind::Amendment => "допсоглашения",
            DossierKind::Act => "акты",
            DossierKind::Evasion => "уклонение от подписания",
        }
    }

    /// Имя папки в архиве досье (FR-1602): латиница - архив уезжает во
    /// внешние системы, где кириллица в путях переживает не всякий распаковщик.
    pub fn folder(self) -> &'static str {
        match self {
            DossierKind::Announcement => "01-announcement",
            DossierKind::Application => "02-applications",
            DossierKind::Protocol => "03-protocols",
            DossierKind::Publication => "04-publication",
            DossierKind::Review => "05-reviews",
            DossierKind::Decision => "06-decisions",
            DossierKind::Contract => "07-contracts",
            DossierKind::Amendment => "10-amendments",
            DossierKind::Act => "08-acts",
            DossierKind::Evasion => "09-evasion",
        }
    }
}

/// Публикация особого порядка (FR-1403, п. 90, 92, 97): что именно
/// университет выкладывает на публичный портал по результатам раздела 12.
/// Перечень закрыт - публикуется то, что названо Правилами, и ничего сверх.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicRecordKind {
    /// Результат рассмотрения заявки: решение Правления с обоснованием (п. 90, 97)
    Decision,
    /// Обоснование ставки договора особого порядка - расчет Прил. 4 (п. 97)
    Rate,
    /// Акт приемки инвестиций (п. 92, FR-1204)
    InvestmentAct,
}

impl PublicRecordKind {
    pub const ALL: [PublicRecordKind; 3] = [
        PublicRecordKind::Decision,
        PublicRecordKind::Rate,
        PublicRecordKind::InvestmentAct,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PublicRecordKind::Decision => "decision",
            PublicRecordKind::Rate => "rate",
            PublicRecordKind::InvestmentAct => "investment_act",
        }
    }

    /// Название раздела публикаций (ru - делопроизводство, NFR-01).
    pub fn title_ru(self) -> &'static str {
        match self {
            PublicRecordKind::Decision => "результаты особого порядка",
            PublicRecordKind::Rate => "обоснования ставок",
            PublicRecordKind::InvestmentAct => "акты приемки инвестиций",
        }
    }

    /// Пункт Правил, по которому публикуется материал.
    pub fn rule_ref(self) -> &'static str {
        match self {
            PublicRecordKind::Decision => "п. 90, 97",
            PublicRecordKind::Rate => "п. 97",
            PublicRecordKind::InvestmentAct => "п. 92",
        }
    }

    /// Публикуется документ, а не запись: у результата и акта это печатная
    /// форма, у обоснования ставки - сам расчет (п. 97, FR-201).
    pub fn needs_document(self) -> bool {
        match self {
            PublicRecordKind::Decision | PublicRecordKind::InvestmentAct => true,
            PublicRecordKind::Rate => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестный вид публикации особого порядка: {0}")]
pub struct UnknownPublicRecordKind(pub String);

impl std::str::FromStr for PublicRecordKind {
    type Err = UnknownPublicRecordKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        PublicRecordKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == s)
            .ok_or_else(|| UnknownPublicRecordKind(s.to_owned()))
    }
}

/// Предмет досье (FR-1206, FR-1602): тендер либо заявка особого порядка -
/// «единое досье по каждому решению» ведется тем же механизмом, что и досье
/// тендера, и отличается от него сроком хранения (INV-042).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DossierSubject {
    Tender,
    SpecialRequest,
}

/// Хранение тендерных материалов - не менее пяти лет (п. 16.15, 42).
pub const TENDER_RETENTION_YEARS: i32 = 5;
/// Хранение решений особого порядка - не менее трех лет (FR-1206, п. 97).
pub const DECISION_RETENTION_YEARS: i32 = 3;

impl DossierSubject {
    pub const ALL: [DossierSubject; 2] = [DossierSubject::Tender, DossierSubject::SpecialRequest];

    /// INV-042: срок WORM-хранения материалов досье. Считается от момента
    /// события, породившего материал (A-075): до его истечения материал
    /// не изымается ни из досье, ни из хранилища.
    pub const fn retention_years(self) -> i32 {
        match self {
            DossierSubject::Tender => TENDER_RETENTION_YEARS,
            DossierSubject::SpecialRequest => DECISION_RETENTION_YEARS,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            DossierSubject::Tender => "tender",
            DossierSubject::SpecialRequest => "special_request",
        }
    }

    /// Название досье (ru - делопроизводство, NFR-01).
    pub fn title_ru(self) -> &'static str {
        match self {
            DossierSubject::Tender => "досье тендера",
            DossierSubject::SpecialRequest => "досье решения особого порядка",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестный вид материала досье: {0}")]
pub struct UnknownDossierKind(pub String);

impl std::str::FromStr for DossierKind {
    type Err = UnknownDossierKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        DossierKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == s)
            .ok_or_else(|| UnknownDossierKind(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_needs_a_document_and_happens_once() {
        let empty = PublicationFacts::default();
        assert_eq!(empty.check_publish(), Err(PublicationError::NoDocument));

        let ready = PublicationFacts {
            has_pdf: true,
            ..empty
        };
        assert_eq!(ready.check_publish(), Ok(()));

        let published = PublicationFacts {
            published: true,
            ..ready
        };
        assert_eq!(
            published.check_publish(),
            Err(PublicationError::AlreadyPublished)
        );
    }

    #[test]
    fn expired_access_is_not_republished() {
        // Снятый протокол публично не возвращается: он в досье (п. 76)
        let expired = PublicationFacts {
            has_pdf: true,
            published: true,
            unpublished: true,
        };
        assert_eq!(
            expired.check_publish(),
            Err(PublicationError::AccessExpired)
        );
        assert!(!expired.is_public());
    }

    #[test]
    fn public_only_between_publication_and_takedown() {
        let published = PublicationFacts {
            has_pdf: true,
            published: true,
            unpublished: false,
        };
        assert!(published.is_public());
        assert!(!PublicationFacts::default().is_public());
    }

    #[test]
    fn dossier_kinds_have_stable_wire_names_and_folders() {
        let mut folders = std::collections::BTreeSet::new();
        for kind in DossierKind::ALL {
            assert_eq!(kind.as_str().parse::<DossierKind>(), Ok(kind));
            assert!(!kind.title_ru().is_empty());
            assert!(
                folders.insert(kind.folder()),
                "папки досье не повторяются: {}",
                kind.folder()
            );
            assert!(kind.folder().is_ascii(), "имя папки архива - латиница");
        }
    }

    #[test]
    fn public_access_lasts_six_months() {
        // INV-076: срок в Правилах - шесть месяцев (п. 76)
        assert_eq!(PUBLIC_ACCESS_MONTHS, 6);
    }

    #[test]
    fn public_records_have_stable_names_and_rule_refs() {
        // FR-1403: перечень публикаций закрыт - п. 90, 92, 97
        let mut names = std::collections::BTreeSet::new();
        for kind in PublicRecordKind::ALL {
            assert_eq!(kind.as_str().parse::<PublicRecordKind>(), Ok(kind));
            assert!(names.insert(kind.as_str()));
            assert!(!kind.title_ru().is_empty());
            assert!(kind.rule_ref().starts_with("п."));
        }
        assert_eq!(
            "выдумка".parse::<PublicRecordKind>(),
            Err(UnknownPublicRecordKind("выдумка".to_owned()))
        );
    }

    #[test]
    fn only_documents_are_required_for_decisions_and_acts() {
        // Решение и акт публикуются печатной формой (как протокол, FR-702),
        // обоснование ставки - самим расчетом Прил. 4
        assert!(PublicRecordKind::Decision.needs_document());
        assert!(PublicRecordKind::InvestmentAct.needs_document());
        assert!(!PublicRecordKind::Rate.needs_document());
    }

    #[test]
    fn inv042_retention_differs_by_subject() {
        // FR-1206: тендерные материалы - 5 лет, решения особого порядка - 3
        assert_eq!(DossierSubject::Tender.retention_years(), 5);
        assert_eq!(DossierSubject::SpecialRequest.retention_years(), 3);
        assert!(
            DossierSubject::Tender.retention_years()
                > DossierSubject::SpecialRequest.retention_years(),
            "материал, попавший в оба досье, хранится по большему сроку"
        );
    }

    #[test]
    fn dossier_subjects_have_stable_wire_names() {
        let mut names = std::collections::BTreeSet::new();
        for subject in DossierSubject::ALL {
            assert!(names.insert(subject.as_str()));
            assert!(subject.as_str().is_ascii());
            assert!(!subject.title_ru().is_empty());
            assert!(subject.retention_years() > 0, "хранение имеет срок");
        }
    }

    #[test]
    fn decision_dossier_has_its_own_sections() {
        // FR-1206: заявка, заключение, решение - разделы досье решения
        for kind in [
            DossierKind::Application,
            DossierKind::Review,
            DossierKind::Decision,
        ] {
            assert_eq!(kind.as_str().parse::<DossierKind>(), Ok(kind));
        }
    }
}
