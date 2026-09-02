# Реестр инвариантов TOU.Rent (INV-###)

Источник: ТЗ v1.0, Приложение В. Каждый инвариант закрепляется на самом
нижнем достижимом уровне: тип → constraint БД → тест (регламент А.5).
Гейт G16 проверяет, что каждый INV упомянут в constraint/типе/тесте
(`testkit/tests/inv_traceability.rs`), а требования контура 3 - что каждое FR
закрыто тестом с привязкой к ID (`testkit/tests/fr_traceability.rs`, T44).

| ID         | Инвариант                                                                        | Уровень закрепления                               | Статус                                                                                                                                                                                                                                           |
| ---------- | -------------------------------------------------------------------------------- | ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| INV-021    | Переходы статусов тендера только по refdata-таблице (п. 5–11)                    | typestate + триггер БД + тест паритета            | готов: триггер (T2), typestate + паритет-тест (T3)                                                                                                                                                                                               |
| INV-037    | Журнал регистрации append-only, seq монотонен, стоп после дедлайна (п. 37–39)    | constraint + триггер БД                           | БД готова (T2); тест testkit - T8                                                                                                                                                                                                                |
| INV-040    | Цены заявок нечитаемы до вскрытия (п. 40–41)                                     | RLS (FORCE, роль без BYPASSRLS) + pgcrypto        | RLS и тест готовы (T2, T8); шифрование ключом тендера - T29 (триггер + тесты)                                                                                                                                                                    |
| INV-042    | Материал досье хранится ≥5 лет (тендер) и ≥3 лет (решение) - WORM (п. 16.15, 42) | тип (`DossierSubject`) + триггер БД + Object Lock | готов (T38): `domain::publication`, триггер `check_dossier_retention`, бакет `dossiers` (compliance), тесты; нижний уровень проверен вживую на прод-compose 10.08.2026 (T75) - `DeleteObject`/`DeleteBucket` ключом приложения дают AccessDenied |
| INV-052    | Основания отклонения - закрытый перечень (п. 52)                                 | refdata-таблица + FK; enum домена                 | таблица + FK готовы (T2); enum домена - T9                                                                                                                                                                                                       |
| INV-055    | Голос члена комиссии ∈ {за, против} (п. 55.8)                                    | тип (enum из 2 вариантов) + триггер права голоса  | готов (T19): `core.vote_value`, `domain::commission::Vote`, триггер `check_vote`                                                                                                                                                                 |
| INV-062    | Старт торгов = max первоначальных предложений допущенных (п. 62)                 | domain + тест                                     | план (T11)                                                                                                                                                                                                                                       |
| INV-063    | Ставка ≥ max + настроенный шаг; шаг ≥5 % от старта (Q-019, п. 63)                | тип + CHECK/триггер БД + domain                   | готов: минимум процента в типе и CHECK, правило ставки в триггере                                                                                                                                                                                |
| INV-066    | Таймер 60 мин, продление ≤1 раза на 15 мин (п. 66, 68)                           | domain (server-authoritative) + триггер БД        | стоп-время и «продление ≤1» в БД (T2); домен - T11                                                                                                                                                                                               |
| INV-076    | Публикация протокола 6 мес, затем снятие (п. 76)                                 | тип + триггер БД + job                            | готов (T28): `domain::publication`, триггер `check_protocol_publication`, джоб снятия, тесты; тот же срок у публикаций особого порядка (T39): триггер `check_public_record`                                                                      |
| INV-086    | Заявка не удовлетворяется при конкуренции (п. 86, 97)                            | тип (`Competition::blocks`) + триггер БД          | готов (T35): `domain::special`, триггер `check_special_competition`, тесты                                                                                                                                                                       |
| INV-087    | Категория особого порядка - закрытый перечень 13 позиций (п. 87)                 | enum домена без catch-all + refdata-таблица + FK  | готов (T33): `domain::special::SpecialCategory`, `refdata.special_categories`, FK + тесты                                                                                                                                                        |
| INV-090    | Решение Правления невозможно без заключения подразделения (п. 89–90)             | тип (`BoardDecision::take`) + триггер БД          | готов (T34): `domain::special`, триггер `special_decision_effects`, тесты                                                                                                                                                                        |
| INV-091    | Инвестиционный договор не подписывается без приложений п. 91                     | refdata-перечень + FK + триггер БД                | готов (T36): `domain::investment::Attachment`, триггер `check_investment_attachments`, тесты                                                                                                                                                     |
| INV-094    | Срок инвестиционного договора не более семи лет (п. 94)                          | тип (`investment::Term`) + CHECK БД               | готов (T36): `domain::investment::Term`, CHECK `term_months`, тесты                                                                                                                                                                              |
| INV-095    | Льгота образовательного оборудования - по согласованию Ученого совета (п. 95)    | тип (`benefit::Benefit::check`) + триггер БД      | готов (T37): `domain::benefit`, триггер `check_benefit_conditions`, тесты                                                                                                                                                                        |
| INV-096    | Спин-офф обучает не менее пяти кредитов в семестр (п. 96)                        | тип (`benefit::Benefit::check`) + триггер БД      | готов (T37): `domain::benefit`, триггер `check_benefit_conditions`, тесты                                                                                                                                                                        |
| INV-105    | Особые условия договора на участок (запрет залога) неизменяемы (п. 107)          | тип (`land::Covenant`) + refdata + триггер БД     | готов (T40): `domain::land`, триггеры `check_land_covenants` и `land_covenants_append_only`, тесты                                                                                                                                               |
| INV-115    | Договор не подписывается без завершенной сверки (п. 113, 115)                    | тип (порядок шагов) + триггер БД                  | готов (T24): `domain::contract`, триггер `enforce_checklist_before_signing`, тест                                                                                                                                                                |
| INV-DB-02  | Нет пересекающихся аренд объекта                                                 | EXCLUDE (gist)                                    | БД готова (T2)                                                                                                                                                                                                                                   |
| INV-DB-05  | Депозитная книга: двойная запись (debit XOR credit), баланс ≥ 0                  | CHECK + триггер                                   | БД готова (T2); операции - контур 2                                                                                                                                                                                                              |
| INV-A01    | Hash-цепочка аудита непрерывна                                                   | триггер + `audit.verify_chain()` + тест G15       | БД готова (T2); тест G15 - контур 2                                                                                                                                                                                                              |
| INV-POL-01 | Политика доступа - исчерпывающий match ролей                                     | тип `Role` (crates/domain), match без catch-all   | готов (T4): `domain::policy` + снапшот-матрица + `CurrentUser::require`                                                                                                                                                                          |

## INV-AUDIT - перечень таблиц с обязательным audit-триггером (FR-1601)

Каждая таблица перечня имеет триггер `audit_record` → `audit.record()`
(миграция `20260806100013_audit_chain.sql`); полноту перечня проверяет тест G15.

`core.tenders` · `core.lots` · `core.applications` · `core.journal_entries` ·
`core.bids` · `core.protocols` · `core.contracts` · `core.ledger_entries` ·
`core.role_grants` · `core.notifications` · `core.user_identities` (T18, FR-1502) ·
`core.commissions` · `core.commission_members` · `core.coi_declarations` ·
`core.member_recusals` · `core.meeting_attendance` · `core.sessions_meetings` ·
`core.votes` (T19, FR-1101–1104) · `core.obligations` (T20, FR-1702) · `core.auction_participants` (T22, FR-604) · `core.contract_checklists` (T24, INV-115) · `core.acts` (T25, FR-904) · `core.evasions` (T26, FR-903) · `core.tender_amendments` (T27, FR-304) · `core.dossier_items` (T28, FR-1602) ·
`core.special_requests` · `core.special_request_files` (T33, FR-1201) ·
`core.special_reviews` · `core.special_board_decisions` (T34, FR-1202) ·
`core.investment_contracts` · `core.investment_contract_files` · `core.investment_acceptances` (T36, FR-1204) ·
`core.benefit_grants` (T37, FR-1205) · `core.public_records` (T39, FR-1403) ·
`core.land_plots` · `core.land_applications` · `core.land_decisions` · `core.land_contracts` · `core.land_contract_covenants` (T40, FR-1801, INV-105) ·
`core.contract_amendments` · `core.contract_amendment_changes` (T42, FR-906) ·
`core.objects` · `core.auctions` · `core.ledger_accounts` (круг 2 гаунтлета:
реестр имущества, итог торгов и лицевые счета мутировались без единого события)

Вне перечня осознанно, обратное направление гейта G15 держит этот список
исключений: `journal_counters` (внутренний счетчик номеров, не факт домена) и
`account_verifications` (одноразовые коды подтверждения регистрации - плумбинг
аутентификации, живет минутами и гасится по `expires_at`). Обе таблицы - схемы
core; в перечне выше их нет намеренно.

## Аудит справочников (T53, FR-1901)

Перечень INV-AUDIT выше - таблицы схемы `core`; его читает тест G15.
Справочники аудируются тем же механизмом, но отдельно от перечня, потому что
у части из них естественные ключи, а не `id uuid`:

`refdata.rate_coefficients` - общий триггер `audit.record()` (есть `id`) ·
`refdata.mrp` (ключ - год) и `refdata.holidays` (ключ - дата) -
`audit.record_natural_key()`: `row_id` пуст, ключ внутри payload.

Причина: МРП и коэффициенты определяют все будущие ставки, календарь - все
процессуальные сроки. Роль приложения не имеет права `DELETE` на `refdata`:
правка справочника - новая версия, а не переписанная история (FR-202).
