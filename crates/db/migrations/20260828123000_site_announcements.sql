-- Управляемое объявление на главной странице. Сейчас у портала одна
-- позиция `home`; отдельная строка с именованной позицией оставляет
-- историю создания/изменения в audit.log и не требует особого singleton-id.
CREATE TABLE core.site_announcements (
  id uuid PRIMARY KEY DEFAULT uuidv7(),
  placement text NOT NULL UNIQUE DEFAULT 'home'
    CHECK (placement = 'home'),
  title text NOT NULL CHECK (char_length(btrim(title)) BETWEEN 1 AND 200),
  body text NOT NULL CHECK (char_length(btrim(body)) BETWEEN 1 AND 20000),
  is_published boolean NOT NULL DEFAULT false,
  published_at timestamptz,
  created_by uuid NOT NULL REFERENCES core.users(id),
  updated_by uuid NOT NULL REFERENCES core.users(id),
  created_at timestamptz NOT NULL DEFAULT core.now(),
  updated_at timestamptz NOT NULL DEFAULT core.now(),
  CHECK ((is_published AND published_at IS NOT NULL)
      OR (NOT is_published AND published_at IS NULL))
);

CREATE TRIGGER audit_record
  AFTER INSERT OR UPDATE OR DELETE ON core.site_announcements
  FOR EACH ROW EXECUTE FUNCTION audit.record();

GRANT SELECT, INSERT, UPDATE ON core.site_announcements TO tou_rent_app;
