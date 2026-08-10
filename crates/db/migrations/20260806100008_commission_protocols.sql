-- Тендерная комиссия, заседания, голоса, протоколы (М5, М7, М11).
-- Составные правила комиссии (нечетность >= 7, кворум 2/3 — FR-1101–1102)
-- закрепляются триггерами в контуре 2; структура — сейчас.

CREATE TABLE core.commissions (
  id          uuid        PRIMARY KEY DEFAULT uuidv7(),
  name        text        NOT NULL,
  valid_from  date        NOT NULL,
  valid_until date        NOT NULL,  -- срок полномочий 1 год (п. 9–11)
  created_at  timestamptz NOT NULL DEFAULT now(),
  CHECK (valid_from < valid_until)
);

CREATE TABLE core.commission_members (
  id            uuid                        PRIMARY KEY DEFAULT uuidv7(),
  commission_id uuid                        NOT NULL REFERENCES core.commissions (id) ON DELETE CASCADE,
  user_id       uuid                        NOT NULL REFERENCES core.users (id),
  member_role   core.commission_member_role NOT NULL DEFAULT 'member',
  UNIQUE (commission_id, user_id)
);
COMMENT ON TABLE core.commission_members IS
  'Секретарь — вне состава комиссии, без голоса (п. 16–17): он не член, а роль secretary';

-- Декларации об отсутствии конфликта интересов до заседания (FR-1104, п. 15)
CREATE TABLE core.coi_declarations (
  id           uuid        PRIMARY KEY DEFAULT uuidv7(),
  member_id    uuid        NOT NULL REFERENCES core.commission_members (id),
  tender_id    uuid        NOT NULL REFERENCES core.tenders (id),
  has_conflict boolean     NOT NULL,
  details      text,
  declared_at  timestamptz NOT NULL DEFAULT now(),
  UNIQUE (member_id, tender_id)
);

CREATE TABLE core.sessions_meetings (
  id            uuid              PRIMARY KEY DEFAULT uuidv7(),
  tender_id     uuid              NOT NULL REFERENCES core.tenders (id),
  commission_id uuid              NOT NULL REFERENCES core.commissions (id),
  kind          core.meeting_kind NOT NULL,
  scheduled_at  timestamptz       NOT NULL,
  held_at       timestamptz,
  UNIQUE (tender_id, kind)
);

-- Голос по заявке: только «за»/«против» (INV-055 закреплен типом core.vote_value).
-- Контур 1: голоса вносит секретарь; контур 2: члены голосуют лично (FR-503, FR-1103).
CREATE TABLE core.votes (
  id             uuid            PRIMARY KEY DEFAULT uuidv7(),
  meeting_id     uuid            NOT NULL REFERENCES core.sessions_meetings (id),
  application_id uuid            NOT NULL REFERENCES core.applications (id),
  member_id      uuid            NOT NULL REFERENCES core.commission_members (id),
  value          core.vote_value NOT NULL,
  dissent        text,           -- особое мнение, прикладывается к протоколу (п. 13–14)
  cast_at        timestamptz     NOT NULL DEFAULT now(),
  UNIQUE (meeting_id, application_id, member_id)
);

-- Протоколы (FR-503, FR-701, FR-802, FR-903): содержимое — jsonb-снимок данных,
-- PDF генерируется Typst и складывается в RustFS.
CREATE TABLE core.protocols (
  id           uuid               PRIMARY KEY DEFAULT uuidv7(),
  tender_id    uuid               NOT NULL REFERENCES core.tenders (id),
  kind         core.protocol_kind NOT NULL,
  meeting_id   uuid               REFERENCES core.sessions_meetings (id),
  number       text,
  content      jsonb              NOT NULL,  -- все поля печатной формы (п. 55, 73–74)
  pdf_key      text,                         -- RustFS
  generated_at timestamptz        NOT NULL DEFAULT now(),
  published_at timestamptz,                  -- публикация в течение 2 р. дней (FR-702)
  unpublish_at timestamptz,                  -- INV-076: снятие через 6 месяцев (джоб apalis)
  UNIQUE (tender_id, kind)
);
