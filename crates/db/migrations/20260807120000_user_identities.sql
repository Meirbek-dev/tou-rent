-- Внешние идентичности (FR-1502, ADR-0003): вход сотрудников через Zitadel
-- (федерация с AD университета). Доменная модель User не меняется — внешний
-- субъект линкуется к существующей core.users по паре (issuer, subject).

CREATE TABLE core.user_identities (
  id            uuid        PRIMARY KEY DEFAULT uuidv7(),
  user_id       uuid        NOT NULL REFERENCES core.users (id) ON DELETE CASCADE,
  issuer        text        NOT NULL,  -- iss из id_token (доверенный провайдер)
  subject       text        NOT NULL,  -- sub: стабилен при смене email
  -- preferred_username провайдера: диагностика и сверка с AD, не ключ
  provider_login text,
  linked_at     timestamptz NOT NULL DEFAULT now(),
  last_login_at timestamptz,
  UNIQUE (issuer, subject)
);

CREATE INDEX user_identities_user_idx ON core.user_identities (user_id);

-- Привязка внешней учетной записи — мутация домена (регламент А.5): в аудит
CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.user_identities
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Происхождение роли (FR-1502 + FR-1503): роль, пришедшая claim'ом провайдера,
-- снимается автоматически при следующем входе без нее; роль, выданную админом
-- вручную, синхронизация провайдера не трогает. Источник истины по правам
-- остается в БД (INV-POL-01), провайдер — лишь один из ее источников.
CREATE TYPE core.role_source AS ENUM ('local', 'oidc');

ALTER TABLE core.role_grants
  ADD COLUMN source core.role_source NOT NULL DEFAULT 'local';
