-- Словарь закрытых перечислений. Единственное место объявления enum-типов БД:
-- тест паритета (G16) сверяет их с Rust-enum'ами crates/domain.

-- Роли (ТЗ § 3, INV-POL-01). `guest` — аноним, в БД не хранится.
CREATE TYPE core.role AS ENUM
  ('participant', 'organizer', 'secretary', 'commission', 'board', 'finance', 'admin');

-- Статусная модель тендера (FR-302); переходы — только по refdata.tender_status_transitions (INV-021)
CREATE TYPE core.tender_status AS ENUM
  ('draft', 'announced', 'accepting', 'qualification', 'trading', 'summed_up',
   'contracted', 'failed', 'repeat_announced', 'cancelled');

-- Тип объекта имущества (FR-101)
CREATE TYPE core.object_kind AS ENUM ('building', 'premises', 'structure', 'land_plot');

-- Заявка участника (М4–М5)
CREATE TYPE core.application_status AS ENUM
  ('submitted', 'withdrawn', 'fee_confirmed', 'admitted', 'rejected');
CREATE TYPE core.applicant_kind AS ENUM ('individual', 'legal_entity');

-- Журнал регистрации (Прил. 12, INV-037)
CREATE TYPE core.journal_entry_kind AS ENUM ('application_submitted', 'application_withdrawn');

-- Комиссия (М11)
CREATE TYPE core.commission_member_role AS ENUM ('chairman', 'deputy', 'member', 'reserve');
CREATE TYPE core.meeting_kind AS ENUM ('qualification', 'results');
-- Голос — только «за»/«против», варианта «воздержался» не существует (INV-055, п. 55.8)
CREATE TYPE core.vote_value AS ENUM ('for', 'against');

-- Протоколы (М7–М8): допуск / итоги / несостоявшийся / победитель № 2
CREATE TYPE core.protocol_kind AS ENUM ('admission', 'results', 'failed', 'winner2');

-- Аукцион (М6)
CREATE TYPE core.auction_status AS ENUM ('scheduled', 'running', 'finished', 'cancelled');

-- Договор (М9); EXCLUDE-периоды действуют в signing/active (INV-DB-02)
CREATE TYPE core.contract_status AS ENUM
  ('draft', 'signing', 'active', 'completed', 'terminated', 'cancelled');

-- Акты (FR-904): прием-передача / возврат
CREATE TYPE core.act_kind AS ENUM ('handover', 'return');

-- Депозитная книга (FR-1001): счета и операции
CREATE TYPE core.ledger_account_kind AS ENUM ('participant_fee', 'contract_deposit');
CREATE TYPE core.ledger_op AS ENUM
  ('receipt_confirmed', 'hold', 'offset', 'refund', 'writeoff', 'replenish');

-- Обязательства-сроки (FR-1702)
CREATE TYPE core.obligation_status AS ENUM ('pending', 'done', 'overdue', 'cancelled');

-- Каналы уведомлений (FR-1301, FR-1303): контур 1 — только in_app;
-- email/telegram добавляются ALTER TYPE ... ADD VALUE в контуре 3
CREATE TYPE core.notification_channel AS ENUM ('in_app');
