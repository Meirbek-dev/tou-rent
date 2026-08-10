//! Реквизиты, которыми сторона называет себя в договоре: ИИН/БИН и телефон
//! (Прил. 2, Прил. 3).
//!
//! Почему в домене, а не в слое http. По ИИН/БИН сторона опознается в договоре
//! и в реестре уклонившихся (FR-1101), по телефону ее извещают (FR-1301).
//! Ошибка в этих реквизитах обнаруживается не при подаче, а при печати
//! договора или при поиске стороны в реестре - то есть после того, как заявка
//! зарегистрирована в журнале (Прил. 12) и стала частью доказательной базы.
//! Значит проверка обязана стоять на входе; а раз так, ей место там, где она
//! доступна без слоя http и попадает под мутационное тестирование: те же
//! правила и тот же перечень примеров держит схема Valibot на фронте
//! (`apps/web/src/lib/validation.ts`), и расхождение между ними - ошибка.
//!
//! # Где проходит граница проверки ИИН/БИН
//!
//! Проверяется ровно то, что задано открытым государственным стандартом
//! и не требует предметных допущений:
//!
//! * двенадцать десятичных цифр;
//! * контрольный разряд по национальному алгоритму РК (веса 1..11; при остатке
//!   10 - второй проход с весами 3,4,5,6,7,8,9,10,11,1,2; остаток 10 и во
//!   втором проходе означает, что номер с такими старшими разрядами не
//!   выдается вовсе).
//!
//! Чего проверка намеренно НЕ делает:
//!
//! * не разделяет ИИН и БИН. Длина, набор символов и алгоритм контрольного
//!   разряда у них общие; различает их только структура старших разрядов
//!   (у ИИН - дата рождения, у БИН - месяц регистрации и признак вида
//!   юрлица). Сверенного с первоисточником описания этой структуры у нас нет,
//!   а цена ошибки несимметрична: лишняя проверка отклоняет действительный
//!   номер живого заявителя, а недостающая лишь пропускает опечатку дальше -
//!   туда, где ее ловит контрольный разряд.
//!   TODO-ENGINEER: сверить структуру ИИН и БИН с первоисточником и решить,
//!   отличать ли их по виду заявителя (`applicant_kind`).
//! * не отличает существующий номер от арифметически правильного - реестры
//!   физических и юридических лиц системе недоступны. Так, `000000000000`
//!   контрольный разряд проходит, хотя такой номер никому не выдан.
//!   TODO-ENGINEER: нужна ли сверка с реестром (ГБД ФЛ/ЮЛ) и на каком шаге -
//!   при подаче заявки или при составлении договора.

/// Число десятичных разрядов в ИИН и в БИН.
pub const ID_NUMBER_LEN: usize = 12;

/// Веса первого прохода: разряды 1..11 умножаются на свой порядковый номер.
const WEIGHTS_FIRST: [u32; ID_NUMBER_LEN - 1] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

/// Веса второго прохода - применяются, когда первый дал остаток 10.
const WEIGHTS_SECOND: [u32; ID_NUMBER_LEN - 1] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 1, 2];

/// Наименьшее и наибольшее число цифр телефона; обоснование - у
/// [`validate_phone`].
pub const PHONE_MIN_DIGITS: usize = 10;
pub const PHONE_MAX_DIGITS: usize = 15;

/// Символы-разделители, которыми номер телефона принято разбивать на группы.
/// Смысла они не несут и на счет цифр не влияют.
const PHONE_SEPARATORS: [char; 4] = [' ', '-', '(', ')'];

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IdNumberError {
    #[error("ИИН/БИН состоит ровно из 12 цифр")]
    Length,
    #[error("ИИН/БИН состоит только из цифр")]
    NotDigits,
    #[error("контрольный разряд ИИН/БИН не сходится - проверьте номер")]
    Checksum,
    /// Оба прохода дали остаток 10: контрольным разрядом может быть только
    /// цифра, поэтому номер с такими старшими разрядами не выдается.
    #[error("такой ИИН/БИН не выдается: контрольного разряда для него не существует")]
    Unassignable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PhoneError {
    #[error("телефон записывается цифрами, допустимы «+», пробел, дефис и скобки")]
    Charset,
    #[error("в номере телефона не меньше {PHONE_MIN_DIGITS} цифр")]
    TooShort,
    #[error("в номере телефона не больше {PHONE_MAX_DIGITS} цифр")]
    TooLong,
}

/// Контрольный разряд номера по одиннадцати старшим цифрам; двенадцатая
/// в расчет не входит (с ней разряд и сверяют).
///
/// `None` - номера с такими старшими разрядами не существует: оба прохода
/// дают остаток 10, а контрольный разряд обязан быть цифрой.
pub fn control_digit(number: &[u8; ID_NUMBER_LEN]) -> Option<u8> {
    // zip обрывается на одиннадцатом весе - двенадцатый разряд в сумму
    // не попадает, отдельной обрезки среза для этого не нужно
    let weighted = |weights: &[u32; ID_NUMBER_LEN - 1]| {
        number
            .iter()
            .zip(weights)
            .map(|(digit, weight)| u32::from(*digit) * weight)
            .sum::<u32>()
            % 11
    };

    let first = weighted(&WEIGHTS_FIRST);
    let remainder = if first == 10 {
        weighted(&WEIGHTS_SECOND)
    } else {
        first
    };

    if remainder == 10 {
        None
    } else {
        u8::try_from(remainder).ok()
    }
}

/// Проверка ИИН либо БИН: 12 цифр и сошедшийся контрольный разряд.
///
/// Пробелы по краям не отбрасываются намеренно: значение сохраняется в заявке
/// как есть и печатается в договоре, поэтому нормализует его форма, а проверка
/// смотрит ровно на то, что будет сохранено.
pub fn validate_id_number(value: &str) -> Result<(), IdNumberError> {
    let digits = parse_digits(value)?;
    let expected = control_digit(&digits).ok_or(IdNumberError::Unassignable)?;

    if digits.last() == Some(&expected) {
        Ok(())
    } else {
        Err(IdNumberError::Checksum)
    }
}

/// Разбор строки в двенадцать цифр.
fn parse_digits(value: &str) -> Result<[u8; ID_NUMBER_LEN], IdNumberError> {
    // Счет по символам, а не по байтам: строка приходит от клиента, и кириллица
    // в ней дала бы длину 24 вместо 12 - то есть неверный диагноз отказа
    if value.chars().count() != ID_NUMBER_LEN {
        return Err(IdNumberError::Length);
    }

    let mut digits = [0_u8; ID_NUMBER_LEN];
    for (slot, symbol) in digits.iter_mut().zip(value.chars()) {
        // to_digit(10) принимает только ASCII-цифры: арабо-индийские и прочие
        // «цифры» Unicode (их пропускает char::is_numeric) в реквизит не годятся
        let digit = symbol.to_digit(10).ok_or(IdNumberError::NotDigits)?;
        *slot = u8::try_from(digit).map_err(|_| IdNumberError::NotDigits)?;
    }
    Ok(digits)
}

/// Проверка телефона.
///
/// Маска, а не формат. Правила не ограничивают круг участников резидентами,
/// поэтому жесткое `+7 7XX XXX XX XX` отклоняло бы действительный номер
/// иностранного заявителя - а это отказ в подаче заявки, цена которого куда
/// выше пропущенной опечатки. Поэтому принимается любая запись из цифр,
/// разделителей и необязательного `+` в начале, а требование одно - число
/// цифр от [`PHONE_MIN_DIGITS`] до [`PHONE_MAX_DIGITS`]: десять - номер РК
/// без кода страны (7XX XXX XX XX), пятнадцать - потолок E.164, длиннее
/// номера не бывает ни в одной стране.
pub fn validate_phone(value: &str) -> Result<(), PhoneError> {
    let trimmed = value.trim();
    // `+` допустим только первым символом: внутри номера это не запись кода
    // страны, а мусор
    let body = trimmed.strip_prefix('+').unwrap_or(trimmed);

    if body
        .chars()
        .any(|symbol| !symbol.is_ascii_digit() && !PHONE_SEPARATORS.contains(&symbol))
    {
        return Err(PhoneError::Charset);
    }

    let digits = body.chars().filter(char::is_ascii_digit).count();
    if digits < PHONE_MIN_DIGITS {
        return Err(PhoneError::TooShort);
    }
    if digits > PHONE_MAX_DIGITS {
        return Err(PhoneError::TooLong);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Общий перечень примеров - он же перечень схем Valibot на фронте
    /// (`apps/web/src/lib/validation.test.ts` читает этот же файл). Паритет
    /// garde ↔ Valibot держится на нем: разошлись правила - разошелся и
    /// результат на одном и том же примере.
    const SAMPLES: &str = include_str!("identity_samples.json");

    fn samples(field: &str) -> Vec<String> {
        let parsed: serde_json::Value = serde_json::from_str(SAMPLES).expect("перечень примеров");
        parsed[field]
            .as_array()
            .expect("массив примеров")
            .iter()
            .map(|value| value.as_str().expect("строка").to_owned())
            .collect()
    }

    #[test]
    fn shared_samples_agree_with_the_check() {
        for value in samples("id_number_valid") {
            assert_eq!(validate_id_number(&value), Ok(()), "ИИН/БИН {value}");
        }
        for value in samples("id_number_invalid") {
            assert!(validate_id_number(&value).is_err(), "ИИН/БИН {value}");
        }
        for value in samples("phone_valid") {
            assert_eq!(validate_phone(&value), Ok(()), "телефон {value}");
        }
        for value in samples("phone_invalid") {
            assert!(validate_phone(&value).is_err(), "телефон {value}");
        }
    }

    /// Контрольный разряд первого прохода: остаток суммы весов 1..11.
    #[test]
    fn control_digit_of_first_pass() {
        // 670124440127: 6+7*2+0+1*4+2*5+4*6+4*7+4*8+0+1*10+2*11 = 150; 150 % 11 = 7
        assert_eq!(validate_id_number("670124440127"), Ok(()));
        assert_eq!(
            validate_id_number("670124440128"),
            Err(IdNumberError::Checksum)
        );
    }

    /// Второй проход: первый дал остаток 10, разряд считается весами
    /// 3,4,5,6,7,8,9,10,11,1,2. Без него номер отвергался бы как ошибочный.
    #[test]
    fn control_digit_falls_back_to_second_pass() {
        for value in ["810203415845", "890904412915", "100650878412"] {
            let digits = parse_digits(value).expect("двенадцать цифр");
            let first = digits
                .iter()
                .zip(&WEIGHTS_FIRST)
                .map(|(digit, weight)| u32::from(*digit) * weight)
                .sum::<u32>()
                % 11;
            assert_eq!(first, 10, "{value} не задействует второй проход");
            assert_eq!(validate_id_number(value), Ok(()), "{value}");
        }
    }

    /// Остаток 10 в обоих проходах: номер недействителен при любой
    /// двенадцатой цифре - подставить «подходящую» нельзя.
    #[test]
    fn number_without_control_digit_is_refused() {
        for prefix in ["62080837541", "65100245280"] {
            for tail in 0..=9 {
                let value = format!("{prefix}{tail}");
                assert_eq!(
                    validate_id_number(&value),
                    Err(IdNumberError::Unassignable),
                    "{value}"
                );
            }
        }
    }

    #[test]
    fn length_and_charset_are_checked_before_the_checksum() {
        assert_eq!(validate_id_number(""), Err(IdNumberError::Length));
        assert_eq!(
            validate_id_number("67012444012"),
            Err(IdNumberError::Length)
        );
        assert_eq!(
            validate_id_number("6701244401270"),
            Err(IdNumberError::Length)
        );
        assert_eq!(
            validate_id_number("67012444012a"),
            Err(IdNumberError::NotDigits)
        );
        // Пробел по краям - тоже символ: значение сохраняется как есть
        assert_eq!(
            validate_id_number(" 670124440127"),
            Err(IdNumberError::Length)
        );
        // Кириллица считается по символам, а не по байтам
        assert_eq!(
            validate_id_number("шестьсот семь"),
            Err(IdNumberError::Length)
        );
        assert_eq!(
            validate_id_number("67012444012ы"),
            Err(IdNumberError::NotDigits)
        );
    }

    /// Граница проверки названа вслух: арифметически верный номер, которого
    /// не существует, проверку проходит (см. модульный комментарий).
    #[test]
    fn checksum_does_not_prove_the_number_exists() {
        assert_eq!(validate_id_number("000000000000"), Ok(()));
    }

    #[test]
    fn phone_accepts_national_and_foreign_records() {
        for value in [
            "87011234567",
            "+77011234567",
            "8 (701) 123-45-67",
            "+7 701 123 45 67",
            "  +49 30 123456  ",
        ] {
            assert_eq!(validate_phone(value), Ok(()), "телефон {value}");
        }
    }

    #[test]
    fn phone_rejects_letters_and_impossible_lengths() {
        assert_eq!(validate_phone("+7 701 12E 45 67"), Err(PhoneError::Charset));
        assert_eq!(
            validate_phone("8-701-123-45-67 доб. 12"),
            Err(PhoneError::Charset)
        );
        // `+` внутри номера - не код страны
        assert_eq!(validate_phone("8701123456+7"), Err(PhoneError::Charset));
        assert_eq!(validate_phone(""), Err(PhoneError::TooShort));
        assert_eq!(validate_phone("+7 (701) 123-45"), Err(PhoneError::TooShort));
        assert_eq!(validate_phone("1234567890123456"), Err(PhoneError::TooLong));
    }

    /// Границы длины принимаются, а соседние значения - нет.
    #[test]
    fn phone_length_boundaries_are_inclusive() {
        let digits = |count: usize| "7".repeat(count);
        assert_eq!(validate_phone(&digits(PHONE_MIN_DIGITS)), Ok(()));
        assert_eq!(validate_phone(&digits(PHONE_MAX_DIGITS)), Ok(()));
        assert_eq!(
            validate_phone(&digits(PHONE_MIN_DIGITS - 1)),
            Err(PhoneError::TooShort)
        );
        assert_eq!(
            validate_phone(&digits(PHONE_MAX_DIGITS + 1)),
            Err(PhoneError::TooLong)
        );
    }
}
