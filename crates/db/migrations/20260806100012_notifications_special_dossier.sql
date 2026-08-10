-- Уведомления in-app (М13), особый порядок (М12, каркас), досье (М16).

-- Центр уведомлений (FR-1301): запись всегда в БД, доставка in-app — SSE.
-- Факт и время создания — доказательная база (FR-1302): таблица в перечне INV-AUDIT.
CREATE TABLE core.notifications (
  id           uuid                      PRIMARY KEY DEFAULT uuidv7(),
  user_id      uuid                      NOT NULL REFERENCES core.users (id),
  channel      core.notification_channel NOT NULL DEFAULT 'in_app',
  kind         text                      NOT NULL,  -- тип события: enum в crates/domain
  payload      jsonb                     NOT NULL DEFAULT '{}',
  created_at   timestamptz               NOT NULL DEFAULT now(),
  delivered_at timestamptz,
  read_at      timestamptz
);

CREATE INDEX notifications_unread_idx ON core.notifications (user_id, created_at DESC)
  WHERE read_at IS NULL;

-- Особый порядок (раздел 12) — каркас контура 3. Категория — text до ввода
-- закрытого enum 13 категорий п. 87 (FR-1201, TODO-ENGINEER: перечень категорий
-- из Правил; enum без catch-all вводится миграцией контура 3).
CREATE TABLE core.special_requests (
  id           uuid        PRIMARY KEY DEFAULT uuidv7(),
  applicant_id uuid        NOT NULL REFERENCES core.users (id),
  category     text        NOT NULL,
  payload      jsonb       NOT NULL DEFAULT '{}',  -- Прил. 3
  status       text        NOT NULL DEFAULT 'submitted',
  created_at   timestamptz NOT NULL DEFAULT now(),
  updated_at   timestamptz NOT NULL DEFAULT now()
);

CREATE TRIGGER touch_updated_at BEFORE UPDATE ON core.special_requests
  FOR EACH ROW EXECUTE FUNCTION core.touch_updated_at();

-- Единое досье (FR-1206, FR-1602): автоматическая сборка документов и событий;
-- хранение >= 5 лет, WORM (Object Lock хранилища) — контур 3.
CREATE TABLE core.dossier_items (
  id                 uuid        PRIMARY KEY DEFAULT uuidv7(),
  tender_id          uuid        REFERENCES core.tenders (id),
  special_request_id uuid        REFERENCES core.special_requests (id),
  kind               text        NOT NULL,   -- вид материала (заявка, протокол, публикация, ...)
  file_key           text,                   -- RustFS (бакет dossiers)
  source_table       text,
  source_id          uuid,
  created_at         timestamptz NOT NULL DEFAULT now(),
  CHECK (tender_id IS NOT NULL OR special_request_id IS NOT NULL)
);
