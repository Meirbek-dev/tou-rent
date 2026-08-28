-- Регистрация внешнего участника по ИИН/БИН с обязательным подтверждением
-- одного канала связи. Поля nullable для служебных и OIDC-аккаунтов.

ALTER TABLE core.users
  ADD COLUMN applicant_kind core.applicant_kind,
  ADD COLUMN id_number text,
  ADD COLUMN phone text,
  ADD COLUMN phone_confirmed_at timestamptz,
  ADD CONSTRAINT participant_id_number_shape CHECK (
    id_number IS NULL OR id_number ~ '^[0-9]{12}$'
  ),
  ADD CONSTRAINT participant_identity_is_complete CHECK (
    (applicant_kind IS NULL AND id_number IS NULL AND phone IS NULL)
    OR (applicant_kind IS NOT NULL AND id_number IS NOT NULL AND phone IS NOT NULL)
  );

CREATE UNIQUE INDEX users_id_number_unique
  ON core.users (id_number) WHERE id_number IS NOT NULL;

CREATE TYPE core.verification_channel AS ENUM ('email', 'sms');

CREATE TABLE core.account_verifications (
  id          uuid                      PRIMARY KEY DEFAULT uuidv7(),
  user_id     uuid                      NOT NULL REFERENCES core.users (id) ON DELETE CASCADE,
  channel     core.verification_channel NOT NULL,
  code_hash   text                      NOT NULL,
  expires_at  timestamptz               NOT NULL,
  consumed_at timestamptz,
  attempts    smallint                  NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 5),
  created_at  timestamptz               NOT NULL DEFAULT core.now(),
  CONSTRAINT verification_expiry_after_creation CHECK (expires_at > created_at)
);

CREATE INDEX account_verifications_active_idx
  ON core.account_verifications (user_id, created_at DESC)
  WHERE consumed_at IS NULL;

COMMENT ON TABLE core.account_verifications IS
  'Одноразовые коды подтверждения; хранится только Argon2id-хеш, срок 15 минут';
