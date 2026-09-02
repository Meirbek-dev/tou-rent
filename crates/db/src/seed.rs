//! Seed-аккаунты ролей (T4, Прил. Б): по одному на каждую роль + три
//! участника-юрлица для демо-сценария § 9.1. Идемпотентно - повторный
//! запуск ничего не ломает. Пароль один на всех, задается SEED_PASSWORD
//! (хеширует вызывающая сторона; в репозитории секретов нет - NFR-09).

use tou_domain::role::Role;

use crate::Db;

/// (email, ФИО, роль). Домен `tou.demo` - заведомо демонстрационный.
pub const ROLE_ACCOUNTS: [(&str, &str, Role); 7] = [
    (
        "organizer@tou.demo",
        "Организатор (юридическая служба)",
        Role::Organizer,
    ),
    (
        "secretary@tou.demo",
        "Секретарь тендерной комиссии",
        Role::Secretary,
    ),
    (
        "commission@tou.demo",
        "Член тендерной комиссии",
        Role::Commission,
    ),
    ("board@tou.demo", "Член Правления", Role::Board),
    (
        "finance@tou.demo",
        "Оператор департамента финансов",
        Role::Finance,
    ),
    (
        "admin@tou.demo",
        "Администратор (цифровое развитие)",
        Role::Admin,
    ),
    (
        "participant@tou.demo",
        "Участник-демонстратор",
        Role::Participant,
    ),
];

/// Дополнительные участники для демо торгов на нескольких экранах (§ 9.1).
pub const EXTRA_PARTICIPANTS: [(&str, &str); 2] = [
    ("participant2@tou.demo", "ТОО «Демо-Участник 2»"),
    ("participant3@tou.demo", "ТОО «Демо-Участник 3»"),
];

/// Демо-комиссия (М11): без нее невозможны заседание, голоса и протокол
/// допуска (FR-503). Состав - по правилам FR-1101: председатель, заместитель,
/// пять членов (голосующих семь, нечетно) и два резервных.
/// TODO-ENGINEER: реальный состав утверждается приказом (п. 9–11).
pub const DEMO_COMMISSION_NAME: &str = "Тендерная комиссия ТОУ (демо, TODO-ENGINEER: состав)";

/// (email, ФИО, роль в комиссии). Отдельные учетные записи, а не носители
/// других ролей: секретарь в состав не входит вовсе (п. 16–17), а Правление
/// и финансы решают свои вопросы.
pub const COMMISSION_ACCOUNTS: [(&str, &str, &str); 9] = [
    (
        "commission1@tou.demo",
        "Председатель тендерной комиссии",
        "chairman",
    ),
    (
        "commission2@tou.demo",
        "Заместитель председателя комиссии",
        "deputy",
    ),
    // Тот же аккаунт, что и в ROLE_ACCOUNTS: демо-вход «член комиссии»
    ("commission@tou.demo", "Член тендерной комиссии", "member"),
    ("commission4@tou.demo", "Член комиссии 4", "member"),
    ("commission5@tou.demo", "Член комиссии 5", "member"),
    ("commission6@tou.demo", "Член комиссии 6", "member"),
    ("commission7@tou.demo", "Член комиссии 7", "member"),
    (
        "commission8@tou.demo",
        "Резервный член комиссии 8",
        "reserve",
    ),
    (
        "commission9@tou.demo",
        "Резервный член комиссии 9",
        "reserve",
    ),
];

/// Идемпотентно приводит демо-комиссию к составу [`COMMISSION_ACCOUNTS`]
/// и утверждает его (FR-1101): срок полномочий - год (п. 9–11). Повторный
/// запуск чинит состав, если его правили руками, и утверждает заново -
/// проверку состава выполняет триггер БД.
pub async fn seed_commission(db: &Db) -> Result<bool, sqlx::Error> {
    let mut tx = db.begin().await?;

    let existing = sqlx::query_scalar!(
        "SELECT id FROM core.commissions WHERE name = $1",
        DEMO_COMMISSION_NAME
    )
    .fetch_optional(&mut *tx)
    .await?;

    let created = existing.is_none();
    let commission_id = match existing {
        Some(id) => id,
        None => {
            sqlx::query_scalar!(
                "INSERT INTO core.commissions (name, valid_from, valid_until)
                 VALUES ($1, (core.now() AT TIME ZONE 'Asia/Almaty')::date, (core.now() AT TIME ZONE 'Asia/Almaty')::date + interval '1 year')
                 RETURNING id",
                DEMO_COMMISSION_NAME
            )
            .fetch_one(&mut *tx)
            .await?
        }
    };

    let emails: Vec<String> = COMMISSION_ACCOUNTS
        .iter()
        .map(|(email, _, _)| (*email).to_owned())
        .collect();

    // Состав контура 1 состоял из носителей других ролей. Проголосовавшего
    // члена удалить нельзя (его голос - часть протокола), поэтому лишние
    // сначала переводятся в резерв (голоса не имеют, в кворум не входят),
    // а затем удаляются те, за кем голосов нет.
    sqlx::query!(
        "UPDATE core.commission_members cm
         SET member_role = 'reserve'
         FROM core.users u
         WHERE cm.commission_id = $1 AND u.id = cm.user_id
           AND u.email::text <> ALL($2::text[])",
        commission_id,
        &emails
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        "DELETE FROM core.commission_members cm
         USING core.users u
         WHERE cm.commission_id = $1 AND u.id = cm.user_id
           AND u.email::text <> ALL($2::text[])
           AND NOT EXISTS (SELECT 1 FROM core.votes v WHERE v.member_id = cm.id)",
        commission_id,
        &emails
    )
    .execute(&mut *tx)
    .await?;

    for (email, _, member_role) in COMMISSION_ACCOUNTS {
        sqlx::query!(
            "INSERT INTO core.commission_members (commission_id, user_id, member_role)
             SELECT $1, u.id, $2::text::core.commission_member_role
             FROM core.users u WHERE u.email = $3::citext
             ON CONFLICT (commission_id, user_id)
             DO UPDATE SET member_role = EXCLUDED.member_role",
            commission_id,
            member_role,
            email
        )
        .execute(&mut *tx)
        .await?;
    }

    // Утверждение состава - проверка FR-1101 на стороне БД (триггер)
    sqlx::query!(
        "UPDATE core.commissions
         SET approved_at = core.now(),
             approved_by = (SELECT id FROM core.users WHERE email = 'admin@tou.demo'::citext)
         WHERE id = $1",
        commission_id
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(created)
}

/// Создает (или пропускает существующие) seed-аккаунты с одним общим
/// `password_hash`. Возвращает количество новых пользователей.
pub async fn seed_accounts(db: &Db, password_hash: &str) -> Result<u32, sqlx::Error> {
    let mut created = 0;

    let accounts = ROLE_ACCOUNTS
        .iter()
        .map(|(email, name, role)| (*email, *name, *role))
        .chain(
            EXTRA_PARTICIPANTS
                .iter()
                .map(|(email, name)| (*email, *name, Role::Participant)),
        )
        // Состав комиссии (FR-1101): у каждого своя учетная запись и роль
        // commission - голосуют они лично (FR-1103)
        .chain(
            COMMISSION_ACCOUNTS
                .iter()
                .map(|(email, name, _)| (*email, *name, Role::Commission)),
        );

    for (email, full_name, role) in accounts {
        let mut tx = db.begin().await?;

        let user_id = sqlx::query_scalar!(
            "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
             VALUES ($1::citext, $2, $3, core.now())
             ON CONFLICT (email) DO NOTHING
             RETURNING id",
            email,
            password_hash,
            full_name
        )
        .fetch_optional(&mut *tx)
        .await?;

        let user_id = match user_id {
            Some(id) => {
                created += 1;
                id
            }
            None => {
                sqlx::query_scalar!("SELECT id FROM core.users WHERE email = $1::citext", email)
                    .fetch_one(&mut *tx)
                    .await?
            }
        };

        // `fetch_one`, а не `execute`: set_config возвращает столбец (см. `set_actor`)
        sqlx::query!(
            "SELECT set_config('app.user_id', $1, true)",
            user_id.to_string()
        )
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query!(
            "INSERT INTO core.role_grants (user_id, role, granted_by)
             VALUES ($1, $2::text::core.role, $1)
             ON CONFLICT (user_id, role) DO NOTHING",
            user_id,
            role.as_str()
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
    }

    Ok(created)
}

/// Демо-объекты (Прил. Б): типовые площади университета. Коды коэффициентов -
/// `default`: реальный каталог опций Прил. 4 в refdata помечен TODO-ENGINEER
/// (первоисточник-PDF недоступен), поэтому расчет ставки идет по базовым.
pub struct DemoObject {
    pub kind: &'static str,
    pub name: &'static str,
    pub address: &'static str,
    /// Площадь, м² (строкой - Decimal не может быть const)
    pub area_m2: &'static str,
    pub floor_part: Option<&'static str>,
    /// Целевое назначение лота на этом объекте
    pub purpose: &'static str,
}

pub const DEMO_OBJECTS: [DemoObject; 6] = [
    DemoObject {
        kind: "premises",
        name: "Аудитория 42 м², корпус А",
        address: "г. Павлодар, ул. Ломова, 64, корпус А",
        area_m2: "42.00",
        floor_part: Some("2 этаж, каб. 214"),
        purpose: "образовательные курсы",
    },
    DemoObject {
        kind: "premises",
        name: "Столовая-буфет, корпус Б",
        address: "г. Павлодар, ул. Ломова, 64, корпус Б",
        area_m2: "120.50",
        floor_part: Some("1 этаж"),
        purpose: "организация питания обучающихся",
    },
    DemoObject {
        kind: "premises",
        name: "Помещение под банкомат 4 м², главный корпус",
        address: "г. Павлодар, ул. Ломова, 64",
        area_m2: "4.00",
        floor_part: Some("1 этаж, холл"),
        purpose: "размещение банкомата",
    },
    DemoObject {
        kind: "premises",
        name: "Коворкинг 85 м², корпус В",
        address: "г. Павлодар, ул. Ломова, 64, корпус В",
        area_m2: "85.00",
        floor_part: Some("3 этаж"),
        purpose: "коворкинг для студенческих проектов",
    },
    DemoObject {
        kind: "premises",
        name: "Спортивный зал 540 м² (почасовая аренда)",
        address: "г. Павлодар, ул. Ломова, 64, спорткомплекс",
        area_m2: "540.00",
        floor_part: None,
        purpose: "спортивные секции (почасово)",
    },
    DemoObject {
        kind: "land_plot",
        name: "Земельный участок 30 м² под торговый киоск",
        address: "г. Павлодар, ул. Ломова, 64, территория кампуса",
        area_m2: "30.00",
        floor_part: None,
        purpose: "торговый киоск",
    },
];

#[derive(Debug, thiserror::Error)]
pub enum SeedError {
    /// Нет МРП или коэффициентов на сегодня - сначала миграции refdata (A-010)
    #[error("refdata не заполнена: {0}")]
    Refdata(String),
    #[error("расчет ставки: {0}")]
    Rate(#[from] tou_domain::rates::RateError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Снимок расчета ставки по Прил. 4 на сегодня с базовыми опциями (FR-201–202).
async fn demo_rate(
    db: &Db,
    area_m2: rust_decimal::Decimal,
) -> Result<tou_domain::rates::RateCalculation, SeedError> {
    use tou_domain::money::Money;
    use tou_domain::rates::{CoefficientCode as C, Factor, RateFactors, RateInputs, calculate};

    let (_, mrp) = crate::refdata::current_mrp(db)
        .await?
        .ok_or_else(|| SeedError::Refdata("МРП на текущий год не заведен".to_owned()))?;
    let table = crate::refdata::coefficients_today(db).await?;

    let factor = |code: C| -> Result<Factor, SeedError> {
        let value = table
            .get(&(code.as_str().to_owned(), "default".to_owned()))
            .copied()
            .ok_or_else(|| {
                SeedError::Refdata(format!("коэффициент {} без опции default", code.as_str()))
            })?;
        Ok(Factor::new("default", value))
    };

    let factors = RateFactors {
        kt: factor(C::Kt)?,
        kk: factor(C::Kk)?,
        ksk: factor(C::Ksk)?,
        kr: factor(C::Kr)?,
        kvd: factor(C::Kvd)?,
        kopf: factor(C::Kopf)?,
        kfu: factor(C::Kfu)?,
        ksots: factor(C::Ksots)?,
        k: factor(C::K)?,
        kn: factor(C::Kn)?,
        kv: factor(C::Kv)?,
    };

    Ok(calculate(RateInputs {
        mrp: Money::new(mrp),
        area_m2,
        factors,
    })?)
}

/// Идемпотентно создает демо-объекты (ключ идемпотентности - имя объекта).
/// Возвращает число созданных.
pub async fn seed_objects(db: &Db) -> Result<u32, SeedError> {
    let mut created = 0;

    for object in &DEMO_OBJECTS {
        // `$n::text::...`: площадь и вид объекта приходят строками (const),
        // а приведение к numeric и перечислению делает БД
        let inserted = sqlx::query_scalar!(
            "INSERT INTO core.objects
               (kind, name, address, area_m2, floor_part,
                premises_type_code, premises_kind_code, comfort_code, location_code)
             SELECT $1::text::core.object_kind, $2, $3, $4::text::numeric, $5,
                    'default', 'default', 'default', 'default'
             WHERE NOT EXISTS (SELECT 1 FROM core.objects WHERE name = $2)
             RETURNING id",
            object.kind,
            object.name,
            object.address,
            object.area_m2,
            object.floor_part
        )
        .fetch_optional(db)
        .await?;

        if inserted.is_some() {
            created += 1;
        }
    }

    Ok(created)
}

/// Реквизиты демо-участников-юрлиц (Прил. Б): `(email, наименование, БИН, цена)`.
/// БИН фиктивный - TODO-ENGINEER: заменить реальными данными перед приемкой.
const DEMO_APPLICANTS: [(&str, &str, &str, &str); 3] = [
    (
        "participant@tou.demo",
        "ТОО «Демо-Участник»",
        "000000000001",
        "55000.00",
    ),
    (
        "participant2@tou.demo",
        "ТОО «Демо-Участник 2»",
        "000000000002",
        "52000.00",
    ),
    (
        "participant3@tou.demo",
        "ТОО «Демо-Участник 3»",
        "000000000003",
        "48000.00",
    ),
];

/// Демо-тендеры портала: по одному в каждом статусе жизненного цикла (Прил. Б).
/// `(заголовок, статус, индекс демо-объекта)`.
const PORTAL_TENDERS: [(&str, &str, usize); 10] = [
    ("Аренда столовой-буфета (черновик)", "draft", 1),
    ("Аренда места под банкомат", "announced", 2),
    ("Аренда коворкинга под студенческие проекты", "accepting", 3),
    ("Почасовая аренда спортивного зала", "qualification", 4),
    (
        "Аренда участка под торговый киоск (идут торги)",
        "trading",
        5,
    ),
    ("Аренда столовой-буфета (итоги подведены)", "summed_up", 1),
    ("Аренда коворкинга (договор заключен)", "contracted", 3),
    ("Аренда столовой-буфета (несостоявшийся)", "failed", 1),
    // repeat_of у повторного тендера в seed не заполняется: связь с исходным
    // ставит процедура повтора (п. 82, контур 2)
    (
        "Аренда места под банкомат (повторный)",
        "repeat_announced",
        2,
    ),
    ("Аренда аудитории (отменен)", "cancelled", 0),
];

/// Заголовок «горячего» тендера демо-сценария § 9.1.
pub const DEMO_TENDER_TITLE: &str = "Аренда аудитории 42 м² (демо-сценарий § 9.1)";

pub struct SeededTenders {
    pub created: u32,
    /// Тендер демо-сценария: заявки поданы, прием закрыт - можно вскрывать
    pub demo_tender_id: Option<uuid::Uuid>,
}

/// Идемпотентно наполняет портал тендерами всех статусов и готовит
/// «горячий» тендер § 9.1: три заявки с разными ценами, прием закрыт,
/// время заседания наступило - демо начинается со вскрытия.
pub async fn seed_tenders(db: &Db) -> Result<SeededTenders, SeedError> {
    let organizer =
        sqlx::query_scalar!("SELECT id FROM core.users WHERE email = 'organizer@tou.demo'::citext")
            .fetch_optional(db)
            .await?;
    let Some(organizer) = organizer else {
        return Err(SeedError::Refdata(
            "нет организатора: сначала seed-аккаунты".to_owned(),
        ));
    };

    let mut created = 0;
    for (title, status, object_index) in PORTAL_TENDERS {
        let Some(object) = DEMO_OBJECTS.get(object_index) else {
            continue;
        };
        if seed_tender(db, organizer, title, status, object).await? {
            created += 1;
        }
    }

    let Some(demo_object) = DEMO_OBJECTS.first() else {
        return Ok(SeededTenders {
            created,
            demo_tender_id: None,
        });
    };
    let demo_created =
        seed_tender(db, organizer, DEMO_TENDER_TITLE, "accepting", demo_object).await?;
    let demo_tender_id = sqlx::query_scalar!(
        "SELECT id FROM core.tenders WHERE title = $1",
        DEMO_TENDER_TITLE
    )
    .fetch_optional(db)
    .await?;

    if demo_created {
        created += 1;
        if let Some(tender_id) = demo_tender_id {
            seed_demo_applications(db, tender_id, DEMO_APPLICANTS.len()).await?;
        }
    }

    Ok(SeededTenders {
        created,
        demo_tender_id,
    })
}

/// Сроки тендера по статусу (`announced_at, submission_deadline, opening_at,
/// opened_at`): публикация в прошлом, прием и вскрытие - до или после
/// «сейчас» в зависимости от стадии. Между публикацией и вскрытием -
/// не меньше 10 календарных дней (FR-303).
/// Вставка тендера с датами, соответствующими статусу.
///
/// Ветвление дает целый запрос, а не кусок списка значений: так текст
/// остается литералом времени компиляции и годится для `query!` (T46).
/// Макрос разворачивается во весь вызов: sqlx принимает последовательность
/// строковых литералов через `+`, и `$dates` подставляется до разбора.
macro_rules! insert_tender {
    ($dates:literal, $title:expr, $organizer:expr, $status:expr) => {
        sqlx::query_scalar!(
            "INSERT INTO core.tenders
               (title, organizer_id, status, announced_at, submission_deadline,
                opening_at, opened_at)
             VALUES ($1, $2, $3::text::core.tender_status, "
                + $dates
                + ")
             RETURNING id",
            $title,
            $organizer,
            $status
        )
    };
}

/// Один демо-тендер с лотом. Ключ идемпотентности - заголовок.
/// Статус задается прямо при вставке: триггер INV-021 стережет UPDATE,
/// а seed воспроизводит уже сложившееся состояние портала.
async fn seed_tender(
    db: &Db,
    organizer: uuid::Uuid,
    title: &str,
    status: &str,
    object: &DemoObject,
) -> Result<bool, SeedError> {
    let exists = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM core.tenders WHERE title = $1) AS "exists!""#,
        title
    )
    .fetch_one(db)
    .await?;
    if exists {
        return Ok(false);
    }

    let area: rust_decimal::Decimal = object
        .area_m2
        .parse()
        .map_err(|_| SeedError::Refdata(format!("площадь объекта {}", object.name)))?;
    let rate = demo_rate(db, area).await?;
    let snapshot = serde_json::to_value(&rate)
        .map_err(|err| SeedError::Refdata(format!("снимок расчета: {err}")))?;
    let monthly = rate.monthly.amount();

    crate::with_actor(db, organizer, async |tx| {
        let tender_id = match status {
            "draft" => {
                insert_tender!("NULL, NULL, NULL, NULL", title, organizer, status)
                    .fetch_one(&mut *tx)
                    .await?
            }
            "announced" | "accepting" => {
                insert_tender!(
                    "core.now() - interval '2 days', core.now() + interval '12 days',
                     core.now() + interval '13 days', NULL",
                    title,
                    organizer,
                    status
                )
                .fetch_one(&mut *tx)
                .await?
            }
            _ => {
                insert_tender!(
                    "core.now() - interval '20 days', core.now() - interval '8 days',
                     core.now() - interval '7 days', core.now() - interval '7 days'",
                    title,
                    organizer,
                    status
                )
                .fetch_one(&mut *tx)
                .await?
            }
        };

        sqlx::query!(
            "INSERT INTO core.lots
               (tender_id, seq, object_id, purpose, lease_months,
                base_rate_monthly, guarantee_fee, rate_calculation)
             SELECT $1, 1, o.id, $2, 12, $3, $3, $4
             FROM core.objects o WHERE o.name = $5",
            tender_id,
            object.purpose,
            monthly,
            snapshot,
            object.name
        )
        .execute(&mut *tx)
        .await?;

        Ok::<_, sqlx::Error>(())
    })
    .await?;

    Ok(true)
}

/// Заявки трех демо-участников с разными ценами (§ 9.1) и записи журнала
/// (Прил. 12). Порядок важен: журнал закрывается дедлайном (INV-037),
/// поэтому прием закрывается уже после подачи.
async fn seed_demo_applications(
    db: &Db,
    tender_id: uuid::Uuid,
    count: usize,
) -> Result<(), SeedError> {
    let lot_id = sqlx::query_scalar!(
        "SELECT id FROM core.lots WHERE tender_id = $1 ORDER BY seq LIMIT 1",
        tender_id
    )
    .fetch_one(db)
    .await?;

    for &(email, name, bin, price) in DEMO_APPLICANTS.iter().take(count) {
        let participant =
            sqlx::query_scalar!("SELECT id FROM core.users WHERE email = $1::citext", email)
                .fetch_optional(db)
                .await?;
        let Some(participant) = participant else {
            continue;
        };

        let details = serde_json::json!({
            "name": name,
            // TODO-ENGINEER: реквизиты демо-стенда фиктивны
            "bin": bin,
            "address": "г. Павлодар, ул. Ломова, 64",
            "phone": "+7 700 000-00-00",
            "email": email,
        });

        crate::with_actor(db, participant, async |tx| {
            let application_id = sqlx::query_scalar!(
                "INSERT INTO core.applications
                   (tender_id, lot_id, participant_id, applicant_kind, applicant_details)
                 VALUES ($1, $2, $3, 'legal_entity', $4) RETURNING id",
                tender_id,
                lot_id,
                participant,
                details
            )
            .fetch_one(&mut *tx)
            .await?;

            sqlx::query!(
                "INSERT INTO core.price_proposals (application_id, amount)
                 VALUES ($1, $2::text::numeric)",
                application_id,
                price
            )
            .execute(&mut *tx)
            .await?;

            sqlx::query!(
                "INSERT INTO core.journal_entries (tender_id, application_id, entry_kind, actor_id)
                 VALUES ($1, $2, 'application_submitted', $3)",
                tender_id,
                application_id,
                participant
            )
            .execute(&mut *tx)
            .await?;

            Ok::<_, sqlx::Error>(())
        })
        .await?;
    }

    // Прием закрыт, время заседания наступило: демо начинается со вскрытия
    sqlx::query!(
        "UPDATE core.tenders
         SET submission_deadline = core.now() - interval '1 hour',
             opening_at = core.now() - interval '30 minutes'
         WHERE id = $1",
        tender_id
    )
    .execute(db)
    .await?;

    Ok(())
}

/// Свежий «горячий» тендер под прогон e2e (T14): та же подготовка, что у
/// демо-тендера § 9.1, но с уникальным заголовком - сценарий можно гонять
/// повторно, не наступая на однократные шаги предыдущего прогона.
/// Возвращает id созданного (или уже существующего с этим заголовком) тендера.
pub async fn seed_demo_tender(db: &Db, title: &str) -> Result<uuid::Uuid, SeedError> {
    let organizer =
        sqlx::query_scalar!("SELECT id FROM core.users WHERE email = 'organizer@tou.demo'::citext")
            .fetch_optional(db)
            .await?;
    let Some(organizer) = organizer else {
        return Err(SeedError::Refdata(
            "нет организатора: сначала seed-аккаунты".to_owned(),
        ));
    };
    let Some(object) = DEMO_OBJECTS.first() else {
        return Err(SeedError::Refdata("нет демо-объектов".to_owned()));
    };

    seed_objects(db).await?;
    let created = seed_tender(db, organizer, title, "accepting", object).await?;

    let tender_id = sqlx::query_scalar!("SELECT id FROM core.tenders WHERE title = $1", title)
        .fetch_one(db)
        .await?;

    if created {
        seed_demo_applications(db, tender_id, DEMO_APPLICANTS.len()).await?;
    }

    Ok(tender_id)
}

/// Тендер, доведенный до итогов торгов, - площадка сценариев контура 2
/// (T32): «полный тендер до договора» и «победитель уклонился → № 2».
///
/// Состояние воспроизводится вставкой, а не прогоном процесса: двое
/// допущены и внесли взносы, торги завершены, победитель и второе место
/// привязаны к реальным ставкам (FR-606). Дальше сценарий идет через UI.
pub async fn seed_summed_up_tender(db: &Db, title: &str) -> Result<uuid::Uuid, SeedError> {
    // Заявки подаются, пока прием открыт (INV-037), поэтому тендер сначала
    // живет как «горячий», а потом доводится до итогов торгов
    let tender_id = seed_demo_tender(db, title).await?;
    // Свой объект на площадку: договор доходит до регистрации, а один объект
    // не сдается на пересекающиеся периоды (INV-DB-02) - иначе повторный
    // прогон сценария упирался бы в чужую аренду
    dedicate_object(db, tender_id, title).await?;
    // Свои участники: сценарий уклонения заносит победителя в реестр
    // уклонистов (FR-505), и демо-аккаунты после него перестали бы
    // проходить допуск в других прогонах
    dedicate_participants(db, tender_id, title).await?;
    advance_to_summed_up(db, tender_id).await?;

    // Допуск: третий участник отклонен основанием п. 52 (FR-502)
    sqlx::query!(
        "UPDATE core.applications a
         SET status = CASE
               WHEN a.applicant_details->>'name' LIKE '%3%' THEN 'rejected'::core.application_status
               ELSE 'admitted'::core.application_status
             END,
             rejection_reason = CASE
               WHEN a.applicant_details->>'name' LIKE '%3%' THEN 'non_compliant'
             END
         WHERE a.tender_id = $1",
        tender_id
    )
    .execute(db)
    .await?;

    seed_admitted_fees(db, tender_id).await?;
    seed_finished_auction(db, tender_id).await?;
    Ok(tender_id)
}

/// Тендер с единственной заявкой и истекшим приемом - площадка сценария
/// «несостоявшийся → повтор» (T32, FR-801: основание п. 81.2).
pub async fn seed_single_application_tender(db: &Db, title: &str) -> Result<uuid::Uuid, SeedError> {
    let organizer =
        sqlx::query_scalar!("SELECT id FROM core.users WHERE email = 'organizer@tou.demo'::citext")
            .fetch_optional(db)
            .await?;
    let Some(organizer) = organizer else {
        return Err(SeedError::Refdata(
            "нет организатора: сначала seed-аккаунты".to_owned(),
        ));
    };
    let Some(object) = DEMO_OBJECTS.first() else {
        return Err(SeedError::Refdata("нет демо-объектов".to_owned()));
    };

    seed_objects(db).await?;
    let created = seed_tender(db, organizer, title, "accepting", object).await?;

    let tender_id = sqlx::query_scalar!("SELECT id FROM core.tenders WHERE title = $1", title)
        .fetch_one(db)
        .await?;

    if created {
        // Одна заявка: журнал append-only, поэтому лишние не удаляются,
        // а просто не подаются (INV-037)
        seed_demo_applications(db, tender_id, 1).await?;
    }

    Ok(tender_id)
}

/// Отдельные участники площадки: заявки переводятся на созданных под этот
/// прогон пользователей (FR-505 - уклонение попадает в реестр навсегда).
async fn dedicate_participants(
    db: &Db,
    tender_id: uuid::Uuid,
    title: &str,
) -> Result<(), SeedError> {
    let applications = sqlx::query!(
        "SELECT id, applicant_details->>'name' AS name FROM core.applications
         WHERE tender_id = $1 ORDER BY submitted_at",
        tender_id
    )
    .fetch_all(db)
    .await?;

    for (index, application) in applications.iter().enumerate() {
        let email = format!(
            "e2e-{}-{}@tou.test",
            index + 1,
            uuid::Uuid::now_v7().simple()
        );
        let full_name = application
            .name
            .clone()
            .unwrap_or_else(|| format!("Участник {}", index + 1));

        let participant = sqlx::query_scalar!(
            "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
             VALUES ($1::citext, 'e2e', $2, core.now()) RETURNING id",
            email,
            format!("{full_name} - {title}")
        )
        .fetch_one(db)
        .await?;

        sqlx::query!(
            "UPDATE core.applications SET participant_id = $2 WHERE id = $1",
            application.id,
            participant
        )
        .execute(db)
        .await?;
    }
    Ok(())
}

/// Отдельный объект имущества для площадки сценария: копия демо-объекта
/// с собственным именем (INV-DB-02).
async fn dedicate_object(db: &Db, tender_id: uuid::Uuid, title: &str) -> Result<(), SeedError> {
    let name = format!("{title} - объект");
    let existing = sqlx::query_scalar!(
        "SELECT o.id FROM core.objects o
         JOIN core.lots l ON l.object_id = o.id
         WHERE l.tender_id = $1 AND o.name = $2",
        tender_id,
        name
    )
    .fetch_optional(db)
    .await?;
    if existing.is_some() {
        return Ok(()); // площадка уже подготовлена этим же заголовком
    }

    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2, floor_part, premises_type_code)
         SELECT o.kind, $2, o.address, o.area_m2, o.floor_part, o.premises_type_code
         FROM core.objects o
         JOIN core.lots l ON l.object_id = o.id
         WHERE l.tender_id = $1
         RETURNING id",
        tender_id,
        name
    )
    .fetch_one(db)
    .await?;

    sqlx::query!(
        "UPDATE core.lots SET object_id = $2 WHERE tender_id = $1",
        tender_id,
        object_id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Площадка сценариев контура 3 (T44): свой объект на прогон и помеченная
/// инвестиционная категория.
///
/// Объект отдельный, потому что сценарий доходит до договора, а один объект
/// не сдается на пересекающиеся периоды (INV-DB-02) - повторный прогон иначе
/// упирался бы в аренду предыдущего (тот же довод, что у площадок контура 2,
/// A-065).
///
/// TODO-ENGINEER: какая из тринадцати категорий п. 87 инвестиционная -
/// вопрос Q-013. Для приемки помечается категория № 10: без пометки правило
/// приоритета большей суммы (FR-1203) не включается ни для одной категории,
/// и инвестиционный договор не заключить.
pub async fn seed_special_site(db: &Db, title: &str) -> Result<uuid::Uuid, SeedError> {
    seed_objects(db).await?;

    let Some(sample) = DEMO_OBJECTS.first() else {
        return Err(SeedError::Refdata("нет демо-объектов".to_owned()));
    };

    let existing = sqlx::query_scalar!("SELECT id FROM core.objects WHERE name = $1", title)
        .fetch_optional(db)
        .await?;

    let object_id = match existing {
        Some(id) => id, // площадка уже подготовлена этим же заголовком
        None => {
            sqlx::query_scalar!(
                "INSERT INTO core.objects
                   (kind, name, address, area_m2, floor_part,
                    premises_type_code, premises_kind_code, comfort_code, location_code)
                 VALUES ($1::text::core.object_kind, $2, $3, $4::text::numeric, $5,
                         'default', 'default', 'default', 'default')
                 RETURNING id",
                sample.kind,
                title,
                sample.address,
                sample.area_m2,
                sample.floor_part
            )
            .fetch_one(db)
            .await?
        }
    };

    // Инвестиционная категория (FR-1203, Q-013): пометка идемпотентна.
    // Справочники Правил ведет владелец схемы, а не роль приложения
    // (у нее на refdata только чтение) - подготовка площадки идет от него
    // и только на время транзакции.
    let mut tx = db.begin().await?;
    sqlx::query!("SET LOCAL ROLE NONE")
        .execute(&mut *tx)
        .await?;
    sqlx::query!(
        "UPDATE refdata.special_categories
         SET competition = 'highest_amount'
         WHERE code = $1 AND competition <> 'highest_amount'",
        INVESTMENT_CATEGORY
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(object_id)
}

/// Категория, помеченная инвестиционной для приемки контура 3 (Q-013).
pub const INVESTMENT_CATEGORY: &str = "category_10";

/// Доведение «горячего» тендера до итогов торгов по разрешенным переходам
/// (INV-021). Заседание допуска создается открытым: seed воспроизводит уже
/// сложившееся состояние, а не проводит заседание заново.
async fn advance_to_summed_up(db: &Db, tender_id: uuid::Uuid) -> Result<(), SeedError> {
    // `!` у status - это `::text`, который планировщик считает потенциально NULL
    let status = sqlx::query_scalar!(
        r#"SELECT status::text AS "status!" FROM core.tenders WHERE id = $1"#,
        tender_id
    )
    .fetch_one(db)
    .await?;
    if status == "summed_up" {
        return Ok(());
    }

    let commission = sqlx::query_scalar!(
        "SELECT id FROM core.commissions
         WHERE approved_at IS NOT NULL AND valid_from <= (core.now() AT TIME ZONE 'Asia/Almaty')::date
           AND (core.now() AT TIME ZONE 'Asia/Almaty')::date < valid_until
         ORDER BY approved_at DESC LIMIT 1"
    )
    .fetch_optional(db)
    .await?;
    let Some(commission) = commission else {
        return Err(SeedError::Refdata(
            "нет утвержденной комиссии: сначала seed комиссии".to_owned(),
        ));
    };

    sqlx::query!(
        "INSERT INTO core.sessions_meetings
           (tender_id, commission_id, kind, scheduled_at, held_at, opened_at,
            quorum_present, quorum_required)
         SELECT $1, $2, 'qualification', core.now() - interval '1 hour',
                core.now() - interval '1 hour', core.now() - interval '1 hour', 7, 5
         WHERE NOT EXISTS (SELECT 1 FROM core.sessions_meetings m
                           WHERE m.tender_id = $1 AND m.kind = 'qualification')",
        tender_id,
        commission
    )
    .execute(db)
    .await?;

    // Каждый шаг - целый запрос, а не статус плюс кусок SQL: так текст
    // остается статическим и проверяемым (T46)
    sqlx::query!(
        "UPDATE core.tenders SET status = 'qualification',
                opened_at = core.now() - interval '30 minutes'
         WHERE id = $1",
        tender_id
    )
    .execute(db)
    .await?;

    sqlx::query!(
        "UPDATE core.tenders SET status = 'trading',
                trading_at = core.now() - interval '20 minutes'
         WHERE id = $1",
        tender_id
    )
    .execute(db)
    .await?;

    sqlx::query!(
        "UPDATE core.tenders SET status = 'summed_up' WHERE id = $1",
        tender_id
    )
    .execute(db)
    .await?;

    Ok(())
}

/// Подтвержденные гарантийные взносы допущенных (FR-405, п. 23): без них
/// при уклонении удерживать нечего (п. 116).
async fn seed_admitted_fees(db: &Db, tender_id: uuid::Uuid) -> Result<(), SeedError> {
    let rows = sqlx::query!(
        "SELECT a.id, a.participant_id, l.guarantee_fee
         FROM core.applications a
         JOIN core.lots l ON l.id = a.lot_id
         WHERE a.tender_id = $1 AND a.status = 'admitted'",
        tender_id
    )
    .fetch_all(db)
    .await?;

    for row in rows {
        let account_id = sqlx::query_scalar!(
            "INSERT INTO core.ledger_accounts (kind, application_id, owner_user_id)
             VALUES ('participant_fee', $1, $2)
             ON CONFLICT (application_id) DO UPDATE SET application_id = EXCLUDED.application_id
             RETURNING id",
            row.id,
            row.participant_id
        )
        .fetch_one(db)
        .await?;

        sqlx::query!(
            "INSERT INTO core.ledger_entries (account_id, op, credit, rule_ref, paid_at)
             SELECT $1, 'receipt_confirmed', $2, 'п. 23, 25', (core.now() AT TIME ZONE 'Asia/Almaty')::date
             WHERE NOT EXISTS (SELECT 1 FROM core.ledger_entries e WHERE e.account_id = $1)",
            account_id,
            row.guarantee_fee
        )
        .execute(db)
        .await?;
    }
    Ok(())
}

/// Завершенные торги лота: ставки допущенных по кругу, победитель и второе
/// место - из реальных ставок (FR-606, INV-063).
async fn seed_finished_auction(db: &Db, tender_id: uuid::Uuid) -> Result<(), SeedError> {
    let lot_id = sqlx::query_scalar!(
        "SELECT id FROM core.lots WHERE tender_id = $1 ORDER BY seq LIMIT 1",
        tender_id
    )
    .fetch_one(db)
    .await?;

    let existing = sqlx::query_scalar!("SELECT id FROM core.auctions WHERE lot_id = $1", lot_id)
        .fetch_optional(db)
        .await?;
    if existing.is_some() {
        return Ok(());
    }

    // Стартовая ставка - максимум первоначальных предложений допущенных
    // (INV-062), шаг - 5 % от нее (INV-063)
    let auction_id = sqlx::query_scalar!(
        "INSERT INTO core.auctions (lot_id, status, starting_bid, bid_step, started_at, ends_at)
         SELECT $1, 'running', s.start, round(s.start * 0.05, 2),
                core.now() - interval '30 minutes', core.now() + interval '1 hour'
         FROM (SELECT max(core.price_amount(p)) AS start
               FROM core.applications a
               JOIN core.price_proposals p ON p.application_id = a.id
               WHERE a.tender_id = $2 AND a.status = 'admitted') s
         RETURNING id",
        lot_id,
        tender_id
    )
    .fetch_one(db)
    .await?;

    // Двое допущенных торгуются по кругу: последний перебил - он победитель
    let admitted = sqlx::query_scalar!(
        "SELECT a.id FROM core.applications a
         WHERE a.tender_id = $1 AND a.status = 'admitted'
         ORDER BY a.submitted_at",
        tender_id
    )
    .fetch_all(db)
    .await?;

    let mut placed: Vec<(uuid::Uuid, rust_decimal::Decimal)> = Vec::new();
    for (index, application_id) in admitted.iter().enumerate() {
        // `$2::int` - номер круга приходит целым, домножение до numeric
        // делает БД; `!` - round() планировщик считает потенциально NULL
        let amount = sqlx::query_scalar!(
            r#"SELECT round(a.starting_bid + a.bid_step * $2::int, 2) AS "amount!"
             FROM core.auctions a WHERE a.id = $1"#,
            auction_id,
            index as i32 + 1
        )
        .fetch_one(db)
        .await?;

        sqlx::query!(
            "INSERT INTO core.bids (id, auction_id, application_id, amount)
             VALUES (uuidv7(), $1, $2, $3)",
            auction_id,
            application_id,
            amount
        )
        .execute(db)
        .await?;
        placed.push((*application_id, amount));
    }

    let Some((winner, winner_amount)) = placed.last().copied() else {
        return Err(SeedError::Refdata(
            "нет допущенных заявок для торгов".to_owned(),
        ));
    };
    let runner_up = placed
        .len()
        .checked_sub(2)
        .and_then(|index| placed.get(index));

    sqlx::query!(
        "UPDATE core.auctions
         SET status = 'finished', finished_at = core.now(),
             winner_application_id = $2, winner_amount = $3,
             runner_up_application_id = $4, runner_up_amount = $5
         WHERE id = $1",
        auction_id,
        winner,
        winner_amount,
        runner_up.map(|(id, _)| *id),
        runner_up.map(|(_, amount)| *amount)
    )
    .execute(db)
    .await?;

    Ok(())
}
