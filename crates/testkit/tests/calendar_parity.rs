//! Гейт G12: паритет производственного календаря (FR-1701).
//!
//! `refdata.add_business_days` (SQL) и `domain::calendar` (Rust) обязаны
//! давать одну и ту же дату - иначе сроки Правил у БД и у приложения
//! разъедутся. Сверка идет на 10⁴ случайных парах «дата + число дней»
//! (ТЗ § 8, G12) плюс на граничных случаях вокруг праздников.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021); тест только читает.

use jiff::civil::Date;
use tou_domain::calendar::BusinessCalendar;

async fn try_pool() -> Result<Option<tou_db::Db>, sqlx::Error> {
    // Пропуск без адреса допустим локально, но не в пайплайне (G2/G15):
    // молча пройденный интеграционный тест ничего не проверяет
    match tou_testkit::database_url().map_err(|e| sqlx::Error::Configuration(Box::new(e)))? {
        Some(url) => tou_db::connect(&url).await.map(Some),
        None => Ok(None),
    }
}

macro_rules! require_db {
    () => {
        match try_pool()
            .await
            .expect("TESTKIT_DATABASE_URL: подключение не удалось")
        {
            Some(db) => db,
            None => {
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - паритет календаря не проверялся");
                return;
            }
        }
    };
}

/// Календарь домена, наполненный теми же праздниками, что и в refdata.
async fn calendar_from_db(db: &tou_db::Db) -> Result<BusinessCalendar, sqlx::Error> {
    let rows = tou_db::obligations::holidays(db).await?;
    let days: Vec<Date> = rows
        .into_iter()
        .filter_map(|(day, _)| to_civil(day))
        .collect();
    Ok(BusinessCalendar::new(days))
}

fn to_civil(date: time::Date) -> Option<Date> {
    Date::new(
        i16::try_from(date.year()).ok()?,
        i8::try_from(u8::from(date.month())).ok()?,
        i8::try_from(date.day()).ok()?,
    )
    .ok()
}

fn to_time(date: Date) -> Option<time::Date> {
    let month = time::Month::try_from(u8::try_from(date.month()).ok()?).ok()?;
    time::Date::from_calendar_date(date.year().into(), month, u8::try_from(date.day()).ok()?).ok()
}

/// Детерминированный генератор: тест обязан падать воспроизводимо.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        // Кнутовский множитель - распределения здесь не требуется, нужна
        // лишь равномерная «разбросанность» дат
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.0 >> 11
    }
}

#[tokio::test]
async fn g12_sql_and_rust_calendars_agree() {
    let db = require_db!();
    let calendar = calendar_from_db(&db).await.expect("праздники из refdata");

    let base = time::Date::from_calendar_date(2026, time::Month::January, 1).expect("дата");
    let mut rng = Lcg(0x5EED_1701);

    // 10⁴ случайных пар: даты в диапазоне ±3 года, сроки 0..30 рабочих дней
    let mut cases: Vec<(time::Date, i32)> = (0..10_000)
        .map(|_| {
            let shift = (rng.next() % 2192) as i64 - 1096;
            let days = (rng.next() % 31) as i32;
            (base + time::Duration::days(shift), days)
        })
        .collect();

    // Граничные случаи: канун Нового года, 8 марта, пятницы и субботы
    for (year, month, day) in [
        (2025, time::Month::December, 31),
        (2026, time::Month::January, 1),
        (2026, time::Month::March, 6),
        (2026, time::Month::March, 7),
        (2026, time::Month::August, 7),
        (2026, time::Month::August, 8),
    ] {
        let date = time::Date::from_calendar_date(year, month, day).expect("дата");
        for days in [0, 1, 2, 3, 5, 10, 15] {
            cases.push((date, days));
        }
    }

    // Одним запросом: 10⁴ round-trip'ов в БД тест бы не пережил
    let (dates, days): (Vec<time::Date>, Vec<i32>) = cases.iter().copied().unzip();
    let expected = sqlx::query_scalar!(
        r#"SELECT refdata.add_business_days(d, n) AS "day!"
           FROM unnest($1::date[], $2::int[]) AS t(d, n)"#,
        &dates,
        &days
    )
    .fetch_all(&db)
    .await
    .expect("SQL-календарь");

    assert_eq!(expected.len(), cases.len());
    for ((start, n), sql) in cases.iter().copied().zip(expected) {
        let civil = to_civil(start).expect("дата домена");
        let rust = calendar.add_business_days(civil, u32::try_from(n).expect("срок"));
        assert_eq!(
            to_time(rust).expect("дата обратно"),
            sql,
            "G12: расхождение календарей на {start} + {n} р. дней"
        );
    }
}
