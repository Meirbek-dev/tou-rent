-- Типизированный обязательный пакет PDF и отметка AES-256-конверта.
-- Старые строки остаются читаемыми после вскрытия как legacy: переписать
-- объект в WORM-бакете нельзя. Все новые заявки получают package_required.

CREATE TYPE core.application_document_kind AS ENUM (
  'application_form',
  'registration_certificate',
  'tax_clearance',
  'guarantee_payment',
  'qualification_documents',
  'legacy'
);

ALTER TABLE core.applications
  ADD COLUMN package_required boolean NOT NULL DEFAULT true;

-- Исторические и seed-заявки создавались до появления обязательного пакета.
UPDATE core.applications SET package_required = false;

ALTER TABLE core.application_files
  ADD COLUMN document_kind core.application_document_kind NOT NULL DEFAULT 'legacy',
  ADD COLUMN encryption_version smallint NOT NULL DEFAULT 0,
  ADD CONSTRAINT application_file_encryption_version
    CHECK (encryption_version IN (0, 1)),
  ADD CONSTRAINT encrypted_application_file_is_pdf
    CHECK (encryption_version = 0 OR content_type = 'application/pdf');

CREATE FUNCTION core.application_package_complete(target_application uuid)
RETURNS boolean
LANGUAGE sql
STABLE
AS $$
  SELECT COALESCE(
    NOT a.package_required OR (
      SELECT count(DISTINCT f.document_kind) = 5
      FROM core.application_files f
      WHERE f.application_id = a.id
        AND f.encryption_version = 1
        AND f.document_kind IN (
          'application_form',
          'registration_certificate',
          'tax_clearance',
          'guarantee_payment',
          'qualification_documents'
        )
    ),
    false
  )
  FROM core.applications a
  WHERE a.id = target_application
$$;

COMMENT ON FUNCTION core.application_package_complete(uuid) IS
  'Полный пакет: пять обязательных типов PDF, зашифрованных AES-256 до записи в RustFS';

CREATE INDEX application_files_kind_idx
  ON core.application_files (application_id, document_kind);

-- Критическое требование ТЗ строже прежнего правила: до вскрытия цена не
-- расшифровывается никому, включая самого участника и администратора сайта.
DROP POLICY sealed_until_opening ON core.price_proposals;
CREATE POLICY sealed_until_opening ON core.price_proposals FOR SELECT
  USING (
    EXISTS (
      SELECT 1
      FROM core.applications a
      JOIN core.tenders t ON t.id = a.tender_id
      WHERE a.id = price_proposals.application_id
        AND t.opened_at IS NOT NULL
    )
  );
