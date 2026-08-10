//! Персональные данные в логах и трейсах (NFR-07, NFR-16).
//!
//! ИИН, БИН, адреса и контакты обязаны отсутствовать в логах и трейсах, но
//! должны полностью фиксироваться в аудите и отдаваться уполномоченному
//! читателю. Обычный комментарий «не логировать» это не гарантирует, поэтому
//! значение оборачивается в [`Redacted`]: `Debug` и `Display` печатают
//! заглушку, а получить содержимое можно только явным [`Redacted::expose`].
//!
//! Граница проходит по слою данных: записи БД держат ПДн за `Redacted`, а
//! в DTO они выходят распакованными - DTO и есть ответ тому, кто вправе их
//! видеть (политика INV-POL-01 и RLS решают, кто это). Сериализация оставлена
//! прозрачной именно поэтому: тип защищает от случайного вывода, а не от
//! осознанной выдачи.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Заглушка вместо значения. Одна и та же строка во всех выводах - по ней
/// ищется утечка в логах.
pub const PLACEHOLDER: &str = "[ПДн скрыты]";

/// Персональные данные: печатаются заглушкой, сериализуются как есть.
#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct Redacted<T>(T);

impl<T> Redacted<T> {
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    /// Явный доступ к значению: вызов видно в диффе и на ревью гейтов.
    pub fn expose(&self) -> &T {
        &self.0
    }

    /// Распаковка на границе слоя (сборка DTO, запись в аудит).
    pub fn into_inner(self) -> T {
        self.0
    }

    pub fn as_ref(&self) -> Redacted<&T> {
        Redacted(&self.0)
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Redacted<U> {
        Redacted(f(self.0))
    }
}

impl<T> From<T> for Redacted<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

// Debug и Display - единственная причина существования типа: ни `?value`,
// ни `%value` в tracing не выведут ПДн.
impl<T> fmt::Debug for Redacted<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(PLACEHOLDER)
    }
}

impl<T> fmt::Display for Redacted<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(PLACEHOLDER)
    }
}

/// Заглушка для ручных реализаций `Debug` там, где поле остается обычным
/// типом (его читает почти каждый вызывающий), а из вывода его нужно убрать.
pub struct Hidden;

impl fmt::Debug for Hidden {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(PLACEHOLDER)
    }
}

impl<T: Serialize> Serialize for Redacted<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Redacted<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IIN: &str = "990101300123";

    #[test]
    fn debug_and_display_hide_the_value() {
        let value = Redacted::new(IIN.to_owned());
        assert_eq!(format!("{value:?}"), PLACEHOLDER);
        assert_eq!(format!("{value}"), PLACEHOLDER);
        assert!(!format!("{value:?} {value}").contains(IIN));
    }

    /// Вложенная структура тоже не течет: производный `Debug` печатает
    /// заглушку вместо поля.
    #[test]
    fn nested_debug_stays_clean() {
        #[derive(Debug)]
        struct Applicant {
            id: u32,
            iin: Redacted<String>,
        }

        let applicant = Applicant {
            id: 7,
            iin: Redacted::new(IIN.to_owned()),
        };
        let printed = format!("{applicant:?}");

        assert!(printed.contains("id: 7"), "{printed}");
        assert!(!printed.contains(IIN), "{printed}");
        // Значение на месте: скрыт вывод, а не сами данные
        assert_eq!(applicant.id, 7);
        assert_eq!(applicant.iin.expose(), IIN);
    }

    /// В ответ уполномоченному читателю значение уходит целиком: тип защищает
    /// от случайного вывода, а не от осознанной выдачи.
    #[test]
    fn serialization_is_transparent() {
        let json = serde_json::to_string(&Redacted::new(IIN.to_owned())).expect("сериализация");
        assert_eq!(json, format!("\"{IIN}\""));

        let back: Redacted<String> = serde_json::from_str(&json).expect("разбор");
        assert_eq!(back.expose(), IIN);
    }

    #[test]
    fn value_is_reachable_only_explicitly() {
        let value = Redacted::new(vec![1, 2, 3]);
        assert_eq!(value.expose().len(), 3);
        assert_eq!(value.into_inner(), vec![1, 2, 3]);
    }
}
