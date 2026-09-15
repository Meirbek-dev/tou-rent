-- Shared audit.record() identifies rows through an id column.
ALTER TABLE core.offline_tender_results ADD COLUMN id uuid GENERATED ALWAYS AS (tender_id) STORED;

CREATE OR REPLACE FUNCTION core.guard_offline_source() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE tid uuid; old_tid uuid;
BEGIN
  IF TG_OP <> 'INSERT' THEN
    IF TG_TABLE_NAME='auctions' THEN
      SELECT tender_id INTO old_tid FROM core.lots WHERE id=OLD.lot_id;
    ELSE
      old_tid := OLD.tender_id;
    END IF;
    PERFORM 1 FROM core.tenders WHERE id=old_tid FOR UPDATE;
    IF EXISTS (SELECT 1 FROM core.offline_tender_results WHERE tender_id=old_tid) THEN
      RAISE EXCEPTION 'FR-801: tender has recorded offline results';
    END IF;
  END IF;
  IF TG_OP='DELETE' THEN RETURN OLD; END IF;
  IF TG_TABLE_NAME='auctions' THEN
    SELECT tender_id INTO tid FROM core.lots WHERE id=NEW.lot_id;
  ELSE
    tid := NEW.tender_id;
  END IF;
  PERFORM 1 FROM core.tenders WHERE id=tid FOR UPDATE;
  IF EXISTS (SELECT 1 FROM core.offline_tender_results WHERE tender_id=tid) THEN
    RAISE EXCEPTION 'FR-801: tender has recorded offline results';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER guard_offline_source_delete BEFORE DELETE ON core.applications
  FOR EACH ROW EXECUTE FUNCTION core.guard_offline_source();
CREATE TRIGGER guard_offline_source_delete BEFORE DELETE ON core.lots
  FOR EACH ROW EXECUTE FUNCTION core.guard_offline_source();

CREATE FUNCTION core.guard_offline_repeat() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.repeat_of IS NOT NULL THEN
    PERFORM 1 FROM core.tenders WHERE id=NEW.repeat_of FOR UPDATE;
    IF EXISTS(SELECT 1 FROM core.offline_tender_results WHERE tender_id=NEW.repeat_of) THEN
      RAISE EXCEPTION 'FR-801: mixed offline results require separate review before repeating lots';
    END IF;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER guard_offline_repeat BEFORE INSERT OR UPDATE OF repeat_of ON core.tenders
  FOR EACH ROW EXECUTE FUNCTION core.guard_offline_repeat();

CREATE FUNCTION core.guard_offline_notification_duty() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.action='notify_admitted' AND NEW.status IN ('pending','overdue','done') THEN
    PERFORM 1 FROM core.tenders WHERE id=NEW.tender_id FOR UPDATE;
    IF EXISTS(SELECT 1 FROM core.offline_tender_results WHERE tender_id=NEW.tender_id) THEN
      RAISE EXCEPTION 'FR-801: auction notification duty is not applicable to an offline result';
    END IF;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER guard_offline_notification_duty BEFORE INSERT OR UPDATE ON core.obligations
  FOR EACH ROW EXECUTE FUNCTION core.guard_offline_notification_duty();
