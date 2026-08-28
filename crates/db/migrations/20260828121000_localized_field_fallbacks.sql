-- Compatibility for trusted imports, seed data and older internal clients:
-- when KK is omitted, keep the record valid by copying RU. The HTTP contract
-- still requires both values for all newly entered public data.
CREATE FUNCTION core.fill_object_kk() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  NEW.name_kk := COALESCE(NULLIF(btrim(NEW.name_kk), ''), NEW.name);
  NEW.address_kk := COALESCE(NULLIF(btrim(NEW.address_kk), ''), NEW.address);
  RETURN NEW;
END $$;

CREATE TRIGGER fill_object_kk BEFORE INSERT OR UPDATE ON core.objects
  FOR EACH ROW EXECUTE FUNCTION core.fill_object_kk();

CREATE FUNCTION core.fill_lot_kk() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  NEW.purpose_kk := COALESCE(NULLIF(btrim(NEW.purpose_kk), ''), NEW.purpose);
  RETURN NEW;
END $$;

CREATE TRIGGER fill_lot_kk BEFORE INSERT OR UPDATE ON core.lots
  FOR EACH ROW EXECUTE FUNCTION core.fill_lot_kk();
