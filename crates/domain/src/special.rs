//! Особый порядок: категории и заявка (М12, FR-1201, п. 87–88).
//!
//! Категория - закрытый перечень из тринадцати позиций п. 87: catch-all
//! запрещен, «прочее» в особом порядке не существует. Наименования категорий
//! и их требования (документы, срок проверки, льготная схема, публикуемость)
//! живут в `refdata.special_categories` - они предметные данные Правил,
//! а не код (INV-087). Здесь закреплено то, что от данных не зависит:
//! перечень номеров, ссылка на подпункт и порядок состояний заявки.
//!
//! TODO-ENGINEER: перечень категорий п. 87 агенту недоступен (Q-009), поэтому
//! варианты названы по номерам - так же, как на них ссылается ТЗ («категории
//! 4–5», FR-1203). Подписи приходят из справочника и правятся без кода.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Категория особого порядка (п. 87): тринадцать позиций, без catch-all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecialCategory {
    Category1,
    Category2,
    Category3,
    Category4,
    Category5,
    Category6,
    Category7,
    Category8,
    Category9,
    Category10,
    Category11,
    Category12,
    Category13,
}

impl SpecialCategory {
    pub const ALL: [SpecialCategory; 13] = [
        SpecialCategory::Category1,
        SpecialCategory::Category2,
        SpecialCategory::Category3,
        SpecialCategory::Category4,
        SpecialCategory::Category5,
        SpecialCategory::Category6,
        SpecialCategory::Category7,
        SpecialCategory::Category8,
        SpecialCategory::Category9,
        SpecialCategory::Category10,
        SpecialCategory::Category11,
        SpecialCategory::Category12,
        SpecialCategory::Category13,
    ];

    /// Номер категории в п. 87 - то, чем ТЗ ее и называет (FR-1203)
    pub fn ordinal(self) -> u8 {
        match self {
            SpecialCategory::Category1 => 1,
            SpecialCategory::Category2 => 2,
            SpecialCategory::Category3 => 3,
            SpecialCategory::Category4 => 4,
            SpecialCategory::Category5 => 5,
            SpecialCategory::Category6 => 6,
            SpecialCategory::Category7 => 7,
            SpecialCategory::Category8 => 8,
            SpecialCategory::Category9 => 9,
            SpecialCategory::Category10 => 10,
            SpecialCategory::Category11 => 11,
            SpecialCategory::Category12 => 12,
            SpecialCategory::Category13 => 13,
        }
    }

    /// Код категории в `refdata.special_categories` (INV-087)
    pub fn as_str(self) -> &'static str {
        match self {
            SpecialCategory::Category1 => "category_1",
            SpecialCategory::Category2 => "category_2",
            SpecialCategory::Category3 => "category_3",
            SpecialCategory::Category4 => "category_4",
            SpecialCategory::Category5 => "category_5",
            SpecialCategory::Category6 => "category_6",
            SpecialCategory::Category7 => "category_7",
            SpecialCategory::Category8 => "category_8",
            SpecialCategory::Category9 => "category_9",
            SpecialCategory::Category10 => "category_10",
            SpecialCategory::Category11 => "category_11",
            SpecialCategory::Category12 => "category_12",
            SpecialCategory::Category13 => "category_13",
        }
    }

    /// Подпункт п. 87 - идет в справочник и в печатную форму заявки
    pub fn rule_ref(self) -> String {
        format!("п. 87.{}", self.ordinal())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестная категория особого порядка: {0}")]
pub struct UnknownCategory(pub String);

impl std::str::FromStr for SpecialCategory {
    type Err = UnknownCategory;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        SpecialCategory::ALL
            .into_iter()
            .find(|category| category.as_str() == s)
            .ok_or_else(|| UnknownCategory(s.to_owned()))
    }
}

/// Льготная схема категории (FR-1205, п. 95–96, Прил. 4). Какой категории
/// какая схема положена - данные справочника; расчет льготы - задача T37.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenefitScheme {
    /// Льгота не применяется
    None,
    /// Оборудование, используемое в образовательном процессе (п. 95)
    EducationalEquipment,
    /// Спин-офф компании университета (п. 96)
    SpinOff,
    /// Социальный арендатор: Ксоц в расчете ставки (Прил. 4, FR-1205)
    Social,
}

impl BenefitScheme {
    pub const ALL: [BenefitScheme; 4] = [
        BenefitScheme::None,
        BenefitScheme::EducationalEquipment,
        BenefitScheme::SpinOff,
        BenefitScheme::Social,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            BenefitScheme::None => "none",
            BenefitScheme::EducationalEquipment => "educational_equipment",
            BenefitScheme::SpinOff => "spin_off",
            BenefitScheme::Social => "social",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестная льготная схема: {0}")]
pub struct UnknownScheme(pub String);

impl std::str::FromStr for BenefitScheme {
    type Err = UnknownScheme;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        BenefitScheme::ALL
            .into_iter()
            .find(|scheme| scheme.as_str() == s)
            .ok_or_else(|| UnknownScheme(s.to_owned()))
    }
}

/// Состояние заявки особого порядка (п. 88–90).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecialRequestStatus {
    /// Подана заявителем; идет срок проверки подразделением (Прил. 3, п. 88–89)
    Submitted,
    /// Проверка проведена, заключение вынесено - заявка на рассмотрении
    /// Правления (п. 89–90). Иного пути в это состояние нет: на нем и стоит
    /// INV-090
    UnderReview,
    /// Правление решило предоставить (п. 90)
    Granted,
    /// Правление отказало (п. 90)
    Refused,
    /// Правление направило вопрос в общий порядок (п. 90, 86)
    Redirected,
    /// Отозвана заявителем до решения
    Withdrawn,
}

impl SpecialRequestStatus {
    pub const ALL: [SpecialRequestStatus; 6] = [
        SpecialRequestStatus::Submitted,
        SpecialRequestStatus::UnderReview,
        SpecialRequestStatus::Granted,
        SpecialRequestStatus::Refused,
        SpecialRequestStatus::Redirected,
        SpecialRequestStatus::Withdrawn,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SpecialRequestStatus::Submitted => "submitted",
            SpecialRequestStatus::UnderReview => "under_review",
            SpecialRequestStatus::Granted => "granted",
            SpecialRequestStatus::Refused => "refused",
            SpecialRequestStatus::Redirected => "redirected",
            SpecialRequestStatus::Withdrawn => "withdrawn",
        }
    }

    /// Решение принято либо заявка отозвана - состояние окончательно
    pub fn is_final(self) -> bool {
        matches!(
            self,
            SpecialRequestStatus::Granted
                | SpecialRequestStatus::Refused
                | SpecialRequestStatus::Redirected
                | SpecialRequestStatus::Withdrawn
        )
    }

    /// Порядок состояний (п. 88–90); тот же перечень стережет триггер БД.
    /// Отозвать заявку можно, пока решение не принято.
    pub fn can_transition_to(self, next: Self) -> bool {
        use SpecialRequestStatus as S;
        match self {
            S::Submitted => matches!(next, S::UnderReview | S::Withdrawn),
            S::UnderReview => {
                matches!(next, S::Granted | S::Refused | S::Redirected | S::Withdrawn)
            }
            S::Granted | S::Refused | S::Redirected | S::Withdrawn => false,
        }
    }
}

/// Решение по заявке особого порядка (п. 90): закрытый перечень, из которого
/// выбирает Правление; тот же перечень - вывод заключения подразделения.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecialDecision {
    /// Предоставить имущество в особом порядке
    Grant,
    /// Отказать
    Refuse,
    /// Направить вопрос в общий порядок (тендер, п. 86)
    Redirect,
}

impl SpecialDecision {
    pub const ALL: [SpecialDecision; 3] = [
        SpecialDecision::Grant,
        SpecialDecision::Refuse,
        SpecialDecision::Redirect,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SpecialDecision::Grant => "grant",
            SpecialDecision::Refuse => "refuse",
            SpecialDecision::Redirect => "redirect",
        }
    }

    /// Состояние заявки после решения (п. 90) - то же соответствие
    /// применяет триггер БД.
    pub fn resulting_status(self) -> SpecialRequestStatus {
        match self {
            SpecialDecision::Grant => SpecialRequestStatus::Granted,
            SpecialDecision::Refuse => SpecialRequestStatus::Refused,
            SpecialDecision::Redirect => SpecialRequestStatus::Redirected,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестное решение по заявке особого порядка: {0}")]
pub struct UnknownDecision(pub String);

impl std::str::FromStr for SpecialDecision {
    type Err = UnknownDecision;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        SpecialDecision::ALL
            .into_iter()
            .find(|decision| decision.as_str() == s)
            .ok_or_else(|| UnknownDecision(s.to_owned()))
    }
}

/// Что делать при двух и более заявках по категории (FR-1203, п. 86, 97).
/// Правило объявляет категория - это данные Правил, а не код.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompetitionRule {
    /// Конкуренция заявок Правилами не ограничена
    #[default]
    None,
    /// Две и более заявки - вопрос выносится в общий порядок (п. 86)
    Redirect,
    /// Приоритет у большей суммы инвестиций; при сопоставимых суммах
    /// решает Правление (п. 97)
    HighestAmount,
}

impl CompetitionRule {
    pub const ALL: [CompetitionRule; 3] = [
        CompetitionRule::None,
        CompetitionRule::Redirect,
        CompetitionRule::HighestAmount,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            CompetitionRule::None => "none",
            CompetitionRule::Redirect => "redirect",
            CompetitionRule::HighestAmount => "highest_amount",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестное правило конкуренции заявок: {0}")]
pub struct UnknownRule(pub String);

impl std::str::FromStr for CompetitionRule {
    type Err = UnknownRule;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        CompetitionRule::ALL
            .into_iter()
            .find(|rule| rule.as_str() == s)
            .ok_or_else(|| UnknownRule(s.to_owned()))
    }
}

/// Обстановка вокруг заявки (п. 86, 97): правило категории, конкуренты
/// на тот же объект и объемы инвестиций.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Competition {
    pub rule: CompetitionRule,
    /// Активные конкурирующие заявки той же категории на тот же объект
    pub rivals: usize,
    /// Наибольший объем инвестиций среди конкурентов
    pub best_rival_amount: Option<Decimal>,
    /// Объем инвестиций самой заявки
    pub own_amount: Option<Decimal>,
    /// Порог сопоставимости сумм в процентах - объявляет категория
    pub comparable_margin_pct: Decimal,
}

impl Competition {
    /// Суммы сопоставимы (п. 97), если отстают от лучшей не более чем
    /// на порог категории. Без конкурентов сравнивать нечего.
    pub fn amounts_comparable(&self) -> bool {
        let Some(best) = self.best_rival_amount else {
            return true;
        };
        let own = self.own_amount.unwrap_or_default();
        if own >= best {
            return true;
        }
        let margin = self.comparable_margin_pct / Decimal::from(100);
        own >= best * (Decimal::ONE - margin)
    }

    /// INV-086: решение, недоступное из-за конкуренции. Отказать и направить
    /// в общий порядок Правление вправе всегда - ограничено «предоставить».
    pub fn blocks(&self, decision: SpecialDecision) -> Option<DecisionError> {
        if decision != SpecialDecision::Grant || self.rivals == 0 {
            return None;
        }
        match self.rule {
            CompetitionRule::None => None,
            CompetitionRule::Redirect => Some(DecisionError::CompetingApplications {
                total: self.rivals + 1,
            }),
            CompetitionRule::HighestAmount if !self.amounts_comparable() => {
                Some(DecisionError::HigherInvestment)
            }
            CompetitionRule::HighestAmount => None,
        }
    }
}

/// Заключение уполномоченного подразделения (п. 89) - вход решения Правления.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conclusion {
    pub recommendation: SpecialDecision,
    /// Состояние заявки на момент заключения: заключение выносится
    /// по поданной заявке (п. 89)
    pub request_status: SpecialRequestStatus,
}

/// Решение Правления как значение (п. 90).
///
/// INV-090: построить его можно только из заключения - без проверки
/// подразделения решения не существует. Второй рубеж - триггер БД,
/// он же ловит вставку мимо приложения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardDecision {
    decision: SpecialDecision,
    recommendation: SpecialDecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DecisionError {
    /// INV-090: заключения подразделения нет (п. 89–90)
    #[error("INV-090: решение Правления невозможно без заключения подразделения (п. 89–90)")]
    NoConclusion,
    /// Заявка отозвана либо решение по ней уже принято (п. 90)
    #[error("решение принимается по заявке, вынесенной на рассмотрение Правления (п. 90)")]
    NotUnderReview,
    /// INV-086: по категории конкурируют несколько заявок - вопрос
    /// выносится в общий порядок (п. 86)
    #[error(
        "INV-086: по категории подано {total} конкурирующих заявок - вопрос выносится в общий порядок (п. 86)"
    )]
    CompetingApplications { total: usize },
    /// INV-086: у конкурента существенно больший объем инвестиций (п. 97)
    #[error("INV-086: конкурирующая заявка предлагает больший объем инвестиций (п. 97)")]
    HigherInvestment,
}

impl BoardDecision {
    /// Решение по заключению. Заявка обязана быть вынесена на рассмотрение
    /// Правления (заключение - единственный путь в это состояние, INV-090),
    /// а конкуренция заявок не должна закрывать выбранное решение (INV-086).
    pub fn take(
        conclusion: Option<Conclusion>,
        request_status: SpecialRequestStatus,
        competition: Competition,
        decision: SpecialDecision,
    ) -> Result<Self, DecisionError> {
        let conclusion = conclusion.ok_or(DecisionError::NoConclusion)?;
        if request_status != SpecialRequestStatus::UnderReview {
            return Err(DecisionError::NotUnderReview);
        }
        if let Some(blocked) = competition.blocks(decision) {
            return Err(blocked);
        }
        Ok(Self {
            decision,
            recommendation: conclusion.recommendation,
        })
    }

    pub fn decision(self) -> SpecialDecision {
        self.decision
    }

    /// Правление вправе решить иначе, чем рекомендовало подразделение
    /// (п. 90): расхождение - не ошибка, но оно видно в протоколе.
    pub fn differs_from_conclusion(self) -> bool {
        self.decision != self.recommendation
    }

    pub fn resulting_status(self) -> SpecialRequestStatus {
        self.decision.resulting_status()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестное состояние заявки особого порядка: {0}")]
pub struct UnknownStatus(pub String);

impl std::str::FromStr for SpecialRequestStatus {
    type Err = UnknownStatus;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        SpecialRequestStatus::ALL
            .into_iter()
            .find(|status| status.as_str() == s)
            .ok_or_else(|| UnknownStatus(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thirteen_categories_numbered_by_the_rules() {
        assert_eq!(SpecialCategory::ALL.len(), 13, "перечень п. 87 закрыт");
        for (index, category) in SpecialCategory::ALL.into_iter().enumerate() {
            let ordinal = u8::try_from(index + 1).expect("номер категории");
            assert_eq!(category.ordinal(), ordinal);
            assert_eq!(category.rule_ref(), format!("п. 87.{ordinal}"));
            assert_eq!(category.as_str().parse::<SpecialCategory>(), Ok(category));
        }
    }

    #[test]
    fn category_codes_are_unique() {
        let mut codes: Vec<&str> = SpecialCategory::ALL
            .iter()
            .map(|category| category.as_str())
            .collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), 13);
    }

    #[test]
    fn wire_names_round_trip() {
        for scheme in BenefitScheme::ALL {
            assert_eq!(scheme.as_str().parse::<BenefitScheme>(), Ok(scheme));
        }
        for status in SpecialRequestStatus::ALL {
            assert_eq!(status.as_str().parse::<SpecialRequestStatus>(), Ok(status));
        }
        assert_eq!(
            "category_14".parse::<SpecialCategory>(),
            Err(UnknownCategory("category_14".to_owned())),
            "четырнадцатой категории в п. 87 нет"
        );
    }

    #[test]
    fn decision_is_final() {
        for status in SpecialRequestStatus::ALL {
            if !status.is_final() {
                continue;
            }
            for next in SpecialRequestStatus::ALL {
                assert!(
                    !status.can_transition_to(next),
                    "{status:?} → {next:?}: решение и отзыв окончательны"
                );
            }
        }
    }

    /// INV-090: без заключения подразделения решение не построить.
    #[test]
    fn inv090_decision_requires_a_conclusion() {
        assert_eq!(
            BoardDecision::take(
                None,
                SpecialRequestStatus::UnderReview,
                Competition::default(),
                SpecialDecision::Grant
            ),
            Err(DecisionError::NoConclusion)
        );

        let conclusion = Conclusion {
            recommendation: SpecialDecision::Grant,
            request_status: SpecialRequestStatus::Submitted,
        };
        let decision = BoardDecision::take(
            Some(conclusion),
            SpecialRequestStatus::UnderReview,
            Competition::default(),
            SpecialDecision::Grant,
        )
        .expect("решение по заключению");
        assert_eq!(decision.decision(), SpecialDecision::Grant);
        assert!(!decision.differs_from_conclusion());
        assert_eq!(
            decision.resulting_status(),
            SpecialRequestStatus::Granted,
            "решение переводит заявку в свое состояние (п. 90)"
        );
    }

    #[test]
    fn decision_needs_a_request_awaiting_the_board() {
        let conclusion = Conclusion {
            recommendation: SpecialDecision::Refuse,
            request_status: SpecialRequestStatus::Submitted,
        };
        for status in [
            SpecialRequestStatus::Submitted,
            SpecialRequestStatus::Withdrawn,
            SpecialRequestStatus::Granted,
        ] {
            assert_eq!(
                BoardDecision::take(
                    Some(conclusion),
                    status,
                    Competition::default(),
                    SpecialDecision::Refuse
                ),
                Err(DecisionError::NotUnderReview),
                "{status:?}"
            );
        }
    }

    /// Правление вправе решить иначе, чем рекомендовало подразделение (п. 90).
    #[test]
    fn board_may_depart_from_the_conclusion() {
        let conclusion = Conclusion {
            recommendation: SpecialDecision::Grant,
            request_status: SpecialRequestStatus::Submitted,
        };
        let decision = BoardDecision::take(
            Some(conclusion),
            SpecialRequestStatus::UnderReview,
            Competition::default(),
            SpecialDecision::Redirect,
        )
        .expect("решение");
        assert!(decision.differs_from_conclusion());
        assert_eq!(
            decision.resulting_status(),
            SpecialRequestStatus::Redirected
        );
    }

    fn conclusion() -> Conclusion {
        Conclusion {
            recommendation: SpecialDecision::Grant,
            request_status: SpecialRequestStatus::Submitted,
        }
    }

    fn competition(rule: CompetitionRule, rivals: usize) -> Competition {
        Competition {
            rule,
            rivals,
            comparable_margin_pct: Decimal::from(5),
            ..Competition::default()
        }
    }

    /// INV-086 (п. 86): две заявки по категории, требующей общего порядка,
    /// закрывают «предоставить» - но не отказ и не перевод в общий порядок.
    #[test]
    fn inv086_competing_applications_close_the_grant() {
        let rivalry = competition(CompetitionRule::Redirect, 1);

        assert_eq!(
            BoardDecision::take(
                Some(conclusion()),
                SpecialRequestStatus::UnderReview,
                rivalry,
                SpecialDecision::Grant
            ),
            Err(DecisionError::CompetingApplications { total: 2 })
        );

        for decision in [SpecialDecision::Refuse, SpecialDecision::Redirect] {
            assert!(
                BoardDecision::take(
                    Some(conclusion()),
                    SpecialRequestStatus::UnderReview,
                    rivalry,
                    decision
                )
                .is_ok(),
                "{decision:?} остается за Правлением"
            );
        }

        // Единственная заявка конкуренцией не ограничена
        assert!(
            BoardDecision::take(
                Some(conclusion()),
                SpecialRequestStatus::UnderReview,
                competition(CompetitionRule::Redirect, 0),
                SpecialDecision::Grant
            )
            .is_ok()
        );
    }

    /// INV-086 (п. 97): приоритет у большей суммы инвестиций.
    #[test]
    fn inv086_higher_investment_wins() {
        let base = Competition {
            rule: CompetitionRule::HighestAmount,
            rivals: 1,
            best_rival_amount: Some(Decimal::from(100_000_000)),
            own_amount: Some(Decimal::from(80_000_000)),
            comparable_margin_pct: Decimal::from(5),
        };

        assert_eq!(
            BoardDecision::take(
                Some(conclusion()),
                SpecialRequestStatus::UnderReview,
                base,
                SpecialDecision::Grant
            ),
            Err(DecisionError::HigherInvestment)
        );

        // Своя сумма больше - приоритет у нее
        let leading = Competition {
            own_amount: Some(Decimal::from(120_000_000)),
            ..base
        };
        assert!(leading.amounts_comparable());
        assert!(
            BoardDecision::take(
                Some(conclusion()),
                SpecialRequestStatus::UnderReview,
                leading,
                SpecialDecision::Grant
            )
            .is_ok()
        );
    }

    /// Сопоставимые суммы приоритета не дают: решает Правление (п. 97).
    #[test]
    fn comparable_amounts_leave_the_choice_to_the_board() {
        let comparable = Competition {
            rule: CompetitionRule::HighestAmount,
            rivals: 1,
            best_rival_amount: Some(Decimal::from(100_000_000)),
            // Отставание 4 % при пороге сопоставимости 5 %
            own_amount: Some(Decimal::from(96_000_000)),
            comparable_margin_pct: Decimal::from(5),
        };
        assert!(comparable.amounts_comparable());
        assert_eq!(comparable.blocks(SpecialDecision::Grant), None);

        // Ровно на границе порога суммы еще сопоставимы
        let edge = Competition {
            own_amount: Some(Decimal::from(95_000_000)),
            ..comparable
        };
        assert!(edge.amounts_comparable());

        // Шаг за порог - приоритет у конкурента
        let below = Competition {
            own_amount: Some(Decimal::from(94_000_000)),
            ..comparable
        };
        assert!(!below.amounts_comparable());
        assert_eq!(
            below.blocks(SpecialDecision::Grant),
            Some(DecisionError::HigherInvestment)
        );
    }

    /// Заявка без суммы проигрывает конкуренту с суммой (п. 97).
    #[test]
    fn a_request_without_an_amount_loses_to_one_with_it() {
        let no_amount = Competition {
            rule: CompetitionRule::HighestAmount,
            rivals: 1,
            best_rival_amount: Some(Decimal::from(30_000_000)),
            own_amount: None,
            comparable_margin_pct: Decimal::from(5),
        };
        assert!(!no_amount.amounts_comparable());
        assert_eq!(
            no_amount.blocks(SpecialDecision::Grant),
            Some(DecisionError::HigherInvestment)
        );
    }

    #[test]
    fn competition_rules_round_trip() {
        for rule in CompetitionRule::ALL {
            assert_eq!(rule.as_str().parse::<CompetitionRule>(), Ok(rule));
        }
        assert_eq!(CompetitionRule::default(), CompetitionRule::None);
    }

    #[test]
    fn decisions_round_trip_and_map_to_statuses() {
        for decision in SpecialDecision::ALL {
            assert_eq!(decision.as_str().parse::<SpecialDecision>(), Ok(decision));
            assert!(decision.resulting_status().is_final());
        }
    }

    #[test]
    fn review_precedes_the_decision() {
        use SpecialRequestStatus as S;

        // Решение Правления невозможно без проверки подразделения (п. 89–90)
        for decision in [S::Granted, S::Refused, S::Redirected] {
            assert!(!S::Submitted.can_transition_to(decision));
            assert!(S::UnderReview.can_transition_to(decision));
        }
        assert!(S::Submitted.can_transition_to(S::UnderReview));

        // Отзыв возможен, пока решения нет
        assert!(S::Submitted.can_transition_to(S::Withdrawn));
        assert!(S::UnderReview.can_transition_to(S::Withdrawn));
    }
}
