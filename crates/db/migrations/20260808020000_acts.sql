-- Акты приема-передачи и возврата (М9, FR-904, Прил. 7–8, п. 122, 128–129).
-- Акт — событие аренды: передача включает начисление платы и делает объект
-- сданным (FR-103), возврат закрывает договор и освобождает объект.

ALTER TABLE core.acts
  ADD COLUMN note        text,
  ADD COLUMN created_by  uuid REFERENCES core.users (id);

-- С даты акта приема-передачи начисляется арендная плата (п. 128–129)
ALTER TABLE core.contracts
  ADD COLUMN rent_starts_on date;

COMMENT ON COLUMN core.contracts.rent_starts_on IS
  'FR-904: дата акта приема-передачи — с нее начисляется арендная плата (п. 122, 128–129)';

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.acts
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Порядок актов (FR-904): передавать можно зарегистрированный договор,
-- возвращать — только переданный объект. Те же правила выражены типом
-- в `domain::act`, здесь — последний рубеж.
CREATE FUNCTION core.check_act_order() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  contract core.contracts%ROWTYPE;
BEGIN
  SELECT * INTO contract FROM core.contracts WHERE id = NEW.contract_id FOR UPDATE;

  IF contract.registered_at IS NULL THEN
    RAISE EXCEPTION 'FR-904: договор не зарегистрирован — передавать объект рано (п. 126)';
  END IF;

  IF NEW.kind = 'return' AND NOT EXISTS (
       SELECT 1 FROM core.acts a
       WHERE a.contract_id = NEW.contract_id AND a.kind = 'handover'
     ) THEN
    RAISE EXCEPTION 'FR-904: объект не передавался — возвращать нечего (п. 129)';
  END IF;

  RETURN NEW;
END $$;

CREATE TRIGGER check_act_order BEFORE INSERT ON core.acts
  FOR EACH ROW EXECUTE FUNCTION core.check_act_order();

-- Следствия акта: передача — договор действует, плата начисляется с даты
-- акта, объект сдан (FR-103); возврат — договор исполнен, период найма
-- закрывается датой возврата и объект снова свободен.
CREATE FUNCTION core.apply_act_effects() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.kind = 'handover' THEN
    UPDATE core.contracts
    SET status = 'active',
        rent_starts_on = NEW.act_date,
        lease_period = tstzrange(
          NEW.act_date::timestamptz,
          NEW.act_date::timestamptz + make_interval(months => coalesce(lease_months, 12)),
          '[)')
    WHERE id = NEW.contract_id;
  ELSE
    UPDATE core.contracts
    SET status = 'completed',
        lease_period = tstzrange(lower(lease_period), NEW.act_date::timestamptz, '[)')
    WHERE id = NEW.contract_id;
  END IF;

  RETURN NULL;  -- AFTER-триггер
END $$;

CREATE TRIGGER apply_act_effects AFTER INSERT ON core.acts
  FOR EACH ROW EXECUTE FUNCTION core.apply_act_effects();

-- Акт — юридический факт: переписать его нельзя (как журнал и ставки)
CREATE TRIGGER acts_append_only BEFORE DELETE ON core.acts
  FOR EACH ROW EXECUTE FUNCTION core.forbid_mutation('FR-904');

REVOKE DELETE ON core.acts FROM tou_rent_app;
