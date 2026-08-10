-- Задел под подпись документов (ТЗ § 2, T63). ЭЦП НУЦ РК вне периметра
-- (ответ № 10): юридически значимый экземпляр - печатная форма плюс скан.
-- Поле хранит способ подписания, чтобы подключение реальной подписи в
-- контуре 5 не переделывало модель договора и акта.

CREATE TYPE core.signature_status AS ENUM (
  'unsigned',    -- подписанного экземпляра нет
  'paper',       -- подписан на бумаге, загружен скан (текущий периметр)
  'electronic'   -- подписан ЭЦП; ставит только провайдер подписи
);

ALTER TABLE core.contracts
  ADD COLUMN signature_status core.signature_status NOT NULL DEFAULT 'unsigned';

ALTER TABLE core.acts
  ADD COLUMN signature_status core.signature_status NOT NULL DEFAULT 'unsigned';

-- Способ подписания - производное от факта, а не отдельно вводимое значение:
-- появился скан - подписан на бумаге, скан снят - снова не подписан.
-- Электронная подпись сильнее бумажной и снятием скана не отменяется.
-- Паритет с `domain::signing::SignatureStatus::with_scan`.
CREATE FUNCTION core.sync_signature_status() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.signature_status = 'electronic' THEN
    RETURN NEW;
  END IF;

  NEW.signature_status := CASE
    WHEN NEW.signed_scan_key IS NOT NULL THEN 'paper'::core.signature_status
    ELSE 'unsigned'::core.signature_status
  END;
  RETURN NEW;
END $$;

CREATE TRIGGER sync_signature_status BEFORE INSERT OR UPDATE ON core.contracts
  FOR EACH ROW EXECUTE FUNCTION core.sync_signature_status();

CREATE TRIGGER sync_signature_status BEFORE INSERT OR UPDATE ON core.acts
  FOR EACH ROW EXECUTE FUNCTION core.sync_signature_status();

-- Уже загруженные сканы: значение выводится из того же правила
UPDATE core.contracts SET signature_status = 'paper' WHERE signed_scan_key IS NOT NULL;
UPDATE core.acts SET signature_status = 'paper' WHERE signed_scan_key IS NOT NULL;
