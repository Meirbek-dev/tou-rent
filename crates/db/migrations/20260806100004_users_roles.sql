-- Пользователи и роли (М15). Контур 1 — email+пароль Argon2id (FR-1501);
-- после миграции на Keycloak (FR-1502) password_hash становится NULL,
-- доменная модель User не меняется.

CREATE TABLE core.users (
  id                 uuid        PRIMARY KEY DEFAULT uuidv7(),
  email              citext      NOT NULL UNIQUE,
  password_hash      text,               -- Argon2id (PHC-строка); NULL после перехода на Keycloak
  full_name          text        NOT NULL,
  locale             text        NOT NULL DEFAULT 'ru' CHECK (locale IN ('kk', 'ru', 'en')),
  email_confirmed_at timestamptz,        -- контур 1: авто-подтверждение, ссылка в лог (FR-1501)
  is_active          boolean     NOT NULL DEFAULT true,
  created_at         timestamptz NOT NULL DEFAULT now(),
  updated_at         timestamptz NOT NULL DEFAULT now()
);

CREATE TRIGGER touch_updated_at BEFORE UPDATE ON core.users
  FOR EACH ROW EXECUTE FUNCTION core.touch_updated_at();

-- Назначение ролей админом (FR-1503); каждое изменение — в аудит (INV-AUDIT).
CREATE TABLE core.role_grants (
  id         uuid        PRIMARY KEY DEFAULT uuidv7(),
  user_id    uuid        NOT NULL REFERENCES core.users (id) ON DELETE CASCADE,
  role       core.role   NOT NULL,
  granted_by uuid        REFERENCES core.users (id),
  granted_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (user_id, role)
);
