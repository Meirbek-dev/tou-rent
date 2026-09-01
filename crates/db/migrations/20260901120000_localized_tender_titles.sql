-- Tender titles are public content, so they need the same RU/KK treatment as
-- object names, addresses and lot purposes. Existing data keeps a safe RU
-- fallback; the imported Rent-lots draft receives its supplied KK heading.
ALTER TABLE core.tenders ADD COLUMN title_kk text;

UPDATE core.tenders
SET title_kk = CASE
  WHEN title = 'Перечень лотов, выставляемых на тендер'
    THEN 'Тендерге шығарылатын лоттар тізбесі'
  ELSE title
END;

ALTER TABLE core.tenders
  ALTER COLUMN title_kk SET NOT NULL,
  ADD CONSTRAINT tenders_title_kk_not_blank CHECK (btrim(title_kk) <> '');

CREATE FUNCTION core.fill_tender_kk() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  NEW.title_kk := COALESCE(NULLIF(btrim(NEW.title_kk), ''), NEW.title);
  RETURN NEW;
END $$;

CREATE TRIGGER fill_tender_kk BEFORE INSERT OR UPDATE ON core.tenders
  FOR EACH ROW EXECUTE FUNCTION core.fill_tender_kk();
