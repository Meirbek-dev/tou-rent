-- Новые значения enum добавлены отдельной предыдущей миграцией: PostgreSQL
-- разрешает безопасно использовать их только после фиксации транзакции.

CREATE OR REPLACE FUNCTION core.application_package_complete(target_application uuid)
RETURNS boolean
LANGUAGE sql
STABLE
AS $$
  SELECT COALESCE(
    NOT a.package_required OR (
      SELECT count(DISTINCT f.document_kind) = 7
      FROM core.application_files f
      WHERE f.application_id = a.id
        AND f.encryption_version = 1
        AND f.document_kind IN (
          'application_form',
          'registration_certificate',
          'tax_clearance',
          'guarantee_payment',
          'qualification_documents',
          'price_proposal_form',
          'qualification_form'
        )
    ),
    false
  )
  FROM core.applications a
  WHERE a.id = target_application
$$;

COMMENT ON FUNCTION core.application_package_complete(uuid) IS
  'Полный пакет: семь обязательных типов PDF, зашифрованных AES-256 до записи в RustFS';
