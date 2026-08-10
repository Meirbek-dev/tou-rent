-- Земельные участки (М18, FR-1801, п. 104–107).
--
-- Порядок раздела 14: университет публикует характеристики участка (под
-- общежития и иное, п. 104) → инвестор подает заявку с проектом и объемом
-- инвестиций (п. 105) → Правление решает (п. 106) → заключается договор
-- с особыми условиями (п. 107).
--
-- INV-105: особые условия договора на участок — запрет залога самого участка
-- и возводимых на нем зданий — защищены дважды: перечень закрыт справочником
-- и типом домена (`land::Covenant`), а триггер не дает ни снять условие,
-- ни подписать договор без полного комплекта. Тот же прием, что у FR-901
-- (существенные условия неизменяемы) и INV-091 (комплект приложений).
--
-- TODO-ENGINEER: раздел 14 Правил агенту недоступен (Q-016). Назначения
-- участков и формулировки особых условий заведены по ТЗ FR-1801 и заведомо
-- черновые; величины (минимальный объем инвестиций, предельный срок)
-- Правилами не заданы и остаются данными заявки.

-- Назначение участка (п. 104): под общежития и иное
CREATE TABLE refdata.land_designations (
  code     text PRIMARY KEY,
  ordinal  int  NOT NULL UNIQUE CHECK (ordinal > 0),
  label_ru text NOT NULL,
  label_kk text,
  label_en text,
  rule_ref text NOT NULL
);

INSERT INTO refdata.land_designations (code, ordinal, label_ru, label_kk, label_en, rule_ref)
VALUES
  ('dormitory', 1, 'Строительство общежития', 'Жатақхана салу',
   'Dormitory construction', 'п. 104'),
  ('other', 2, 'Иное назначение (уточняется по Правилам)',
   'Өзге мақсат (Ережелер бойынша нақтыланады)', 'Other purpose (to be confirmed)', 'п. 104')
ON CONFLICT DO NOTHING;

-- INV-105 (п. 107): закрытый перечень особых условий договора на участок
CREATE TABLE refdata.land_covenants (
  code     text PRIMARY KEY,
  ordinal  int  NOT NULL UNIQUE CHECK (ordinal > 0),
  label_ru text NOT NULL,
  label_kk text,
  label_en text,
  rule_ref text NOT NULL
);

COMMENT ON TABLE refdata.land_covenants IS
  'INV-105 (FR-1801, п. 107): особые условия договора на земельный участок — закрытый перечень';

INSERT INTO refdata.land_covenants (code, ordinal, label_ru, label_kk, label_en, rule_ref)
VALUES
  ('no_pledge_plot', 1, 'Запрет залога земельного участка',
   'Жер учаскесін кепілге қоюға тыйым', 'The plot may not be pledged', 'п. 107'),
  ('no_pledge_buildings', 2, 'Запрет залога возводимых на участке зданий',
   'Учаскеде салынатын ғимараттарды кепілге қоюға тыйым',
   'Buildings erected on the plot may not be pledged', 'п. 107')
ON CONFLICT DO NOTHING;

-- Характеристики участка (п. 104): объект реестра плюс то, что Правила
-- требуют публиковать по участку. Публикация — состояние со сроком начала:
-- заявки принимаются только по опубликованному участку (п. 105).
CREATE TABLE core.land_plots (
  -- Суррогатный ключ: его требует audit-триггер (INV-AUDIT), а участок
  -- остается один на объект
  id             uuid          PRIMARY KEY DEFAULT uuidv7(),
  object_id      uuid          NOT NULL UNIQUE REFERENCES core.objects (id),
  cadastral_number text        NOT NULL,
  designation    text          NOT NULL REFERENCES refdata.land_designations (code),
  permitted_use  text          NOT NULL,
  -- Ожидаемый объем инвестиций, если университет его объявляет (п. 104)
  min_investment numeric(14,2) CHECK (min_investment > 0),
  published_at   timestamptz,
  created_at     timestamptz   NOT NULL DEFAULT now(),
  updated_at     timestamptz   NOT NULL DEFAULT now(),
  CONSTRAINT land_plot_cadastral_not_empty CHECK (length(btrim(cadastral_number)) > 0),
  CONSTRAINT land_plot_permitted_use_not_empty CHECK (length(btrim(permitted_use)) > 0)
);

COMMENT ON TABLE core.land_plots IS
  'FR-1801 (п. 104): характеристики земельного участка; публикуются на портале';

CREATE TRIGGER touch_updated_at BEFORE UPDATE ON core.land_plots
  FOR EACH ROW EXECUTE FUNCTION core.touch_updated_at();

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.land_plots
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Характеристики участка описывают участок, а не помещение (FR-101)
CREATE FUNCTION core.check_land_plot_object() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  object_kind core.object_kind;
BEGIN
  SELECT kind INTO object_kind FROM core.objects WHERE id = NEW.object_id;
  IF object_kind IS DISTINCT FROM 'land_plot' THEN
    RAISE EXCEPTION
      'FR-1801: характеристики раздела 14 заводятся на земельный участок (п. 104)'
      USING ERRCODE = 'raise_exception';
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER check_land_plot_object BEFORE INSERT OR UPDATE ON core.land_plots
  FOR EACH ROW EXECUTE FUNCTION core.check_land_plot_object();

-- Заявка инвестора (п. 105)
CREATE TYPE core.land_application_status AS ENUM
  ('submitted', 'granted', 'refused', 'withdrawn');

CREATE TABLE core.land_applications (
  id                uuid                        PRIMARY KEY DEFAULT uuidv7(),
  plot_id           uuid                        NOT NULL REFERENCES core.land_plots (object_id),
  investor_id       uuid                        NOT NULL REFERENCES core.users (id),
  project           text                        NOT NULL,
  investment_amount numeric(14,2)               NOT NULL CHECK (investment_amount > 0),
  term_months       int                         NOT NULL CHECK (term_months > 0),
  status            core.land_application_status NOT NULL DEFAULT 'submitted',
  submitted_at      timestamptz                 NOT NULL DEFAULT now(),
  withdrawn_at      timestamptz,
  CONSTRAINT land_application_project_not_empty CHECK (length(btrim(project)) > 0),
  CONSTRAINT land_application_withdrawal_has_timestamp
    CHECK (status <> 'withdrawn' OR withdrawn_at IS NOT NULL)
);

COMMENT ON TABLE core.land_applications IS
  'FR-1801 (п. 105): заявка инвестора на земельный участок — проект, объем инвестиций и срок';

CREATE INDEX land_applications_plot_idx ON core.land_applications (plot_id, submitted_at DESC);
CREATE INDEX land_applications_investor_idx
  ON core.land_applications (investor_id, submitted_at DESC);

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.land_applications
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Заявка подается по опубликованному участку (п. 104–105): непубличный
-- участок для инвестора не существует.
CREATE FUNCTION core.check_land_application() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM core.land_plots p
    WHERE p.object_id = NEW.plot_id AND p.published_at IS NOT NULL
  ) THEN
    RAISE EXCEPTION
      'FR-1801: заявка подается по опубликованному участку (п. 104–105)'
      USING ERRCODE = 'raise_exception';
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER check_land_application BEFORE INSERT ON core.land_applications
  FOR EACH ROW EXECUTE FUNCTION core.check_land_application();

-- Порядок состояний (п. 105–106): решение и отзыв окончательны
CREATE FUNCTION core.check_land_application_transition() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.status = OLD.status THEN
    RETURN NEW;
  END IF;

  IF OLD.status <> 'submitted' THEN
    RAISE EXCEPTION 'FR-1801: переход заявки на участок % → % запрещен (п. 105–106)',
      OLD.status, NEW.status;
  END IF;

  IF NEW.status = 'withdrawn' AND NEW.withdrawn_at IS NULL THEN
    NEW.withdrawn_at := now();  -- время отзыва задает сервер (NFR-03)
  END IF;

  RETURN NEW;
END $$;

CREATE TRIGGER check_land_application_transition BEFORE UPDATE ON core.land_applications
  FOR EACH ROW EXECUTE FUNCTION core.check_land_application_transition();

-- Решение Правления по заявке (п. 106): одно на заявку, с обоснованием
CREATE TYPE core.land_decision AS ENUM ('grant', 'refuse');

CREATE TABLE core.land_decisions (
  id                  uuid               PRIMARY KEY DEFAULT uuidv7(),
  land_application_id uuid               NOT NULL UNIQUE REFERENCES core.land_applications (id),
  decision            core.land_decision NOT NULL,
  rationale           text               NOT NULL,
  decided_by          uuid               NOT NULL REFERENCES core.users (id),
  decided_at          timestamptz        NOT NULL DEFAULT now(),
  CONSTRAINT land_decision_rationale_not_empty CHECK (length(btrim(rationale)) > 0)
);

COMMENT ON TABLE core.land_decisions IS
  'FR-1801 (п. 106): решение Правления по заявке на земельный участок с обоснованием';

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.land_decisions
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Решение — юридический факт: его не переписывают и не удаляют (п. 106)
CREATE TRIGGER land_decisions_append_only BEFORE UPDATE OR DELETE ON core.land_decisions
  FOR EACH ROW EXECUTE FUNCTION core.forbid_mutation('FR-1801');

REVOKE UPDATE, DELETE ON core.land_decisions FROM tou_rent_app;

-- Принятое решение переводит заявку в свое терминальное состояние (п. 106)
CREATE FUNCTION core.land_decision_effects() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  next_status core.land_application_status;
BEGIN
  next_status := CASE NEW.decision
    WHEN 'grant'  THEN 'granted'
    WHEN 'refuse' THEN 'refused'
  END::core.land_application_status;

  UPDATE core.land_applications
     SET status = next_status
   WHERE id = NEW.land_application_id AND status = 'submitted';

  IF NOT FOUND THEN
    RAISE EXCEPTION
      'FR-1801: решение принимается по поданной заявке на участок (п. 105–106)'
      USING ERRCODE = 'raise_exception';
  END IF;

  RETURN NULL;
END $$;

CREATE TRIGGER land_decision_effects AFTER INSERT ON core.land_decisions
  FOR EACH ROW EXECUTE FUNCTION core.land_decision_effects();

-- Договор на участок (п. 107): живет в общей таблице договоров (объект,
-- наниматель, ставка, INV-DB-02 — там же), своим остается инвестиционная
-- часть и особые условия.
CREATE TABLE core.land_contracts (
  id                  uuid          PRIMARY KEY DEFAULT uuidv7(),
  contract_id         uuid          NOT NULL UNIQUE REFERENCES core.contracts (id),
  land_application_id uuid          NOT NULL UNIQUE REFERENCES core.land_applications (id),
  -- Снимок заявки: объем инвестиций — существенное условие (FR-901)
  investment_amount   numeric(14,2) NOT NULL CHECK (investment_amount > 0),
  created_at          timestamptz   NOT NULL DEFAULT now()
);

COMMENT ON TABLE core.land_contracts IS
  'FR-1801 (п. 107): договор на земельный участок; особые условия — INV-105';

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.land_contracts
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Договор заключается по удовлетворенной заявке (п. 106–107)
CREATE FUNCTION core.check_land_contract() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  application_status core.land_application_status;
BEGIN
  SELECT status INTO application_status
  FROM core.land_applications WHERE id = NEW.land_application_id;

  IF application_status IS DISTINCT FROM 'granted' THEN
    RAISE EXCEPTION
      'FR-1801: договор заключается по удовлетворенной заявке на участок (п. 106–107)'
      USING ERRCODE = 'raise_exception';
  END IF;

  RETURN NEW;
END $$;

CREATE TRIGGER check_land_contract BEFORE INSERT ON core.land_contracts
  FOR EACH ROW EXECUTE FUNCTION core.check_land_contract();

-- INV-105: особые условия договора (п. 107). Перечень закрыт справочником,
-- условие нельзя ни переписать, ни снять — только внести.
CREATE TABLE core.land_contract_covenants (
  id          uuid        PRIMARY KEY DEFAULT uuidv7(),
  contract_id uuid        NOT NULL REFERENCES core.contracts (id),
  code        text        NOT NULL REFERENCES refdata.land_covenants (code),
  created_at  timestamptz NOT NULL DEFAULT now(),
  UNIQUE (contract_id, code)
);

COMMENT ON TABLE core.land_contract_covenants IS
  'INV-105 (п. 107): особые условия договора на участок — вносятся и не снимаются';

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.land_contract_covenants
  FOR EACH ROW EXECUTE FUNCTION audit.record();

CREATE TRIGGER land_covenants_append_only
  BEFORE UPDATE OR DELETE ON core.land_contract_covenants
  FOR EACH ROW EXECUTE FUNCTION core.forbid_mutation('INV-105');

REVOKE UPDATE, DELETE ON core.land_contract_covenants FROM tou_rent_app;

-- INV-105: договор на участок не подписывается без полного комплекта особых
-- условий — тот же рубеж, что INV-091 и INV-115 (проверка на переходе
-- в signing: до подписания условия можно вносить).
CREATE FUNCTION core.check_land_covenants() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  missing int;
BEGIN
  IF NEW.status <> 'signing' OR OLD.status = 'signing' THEN
    RETURN NEW;
  END IF;

  IF NOT EXISTS (SELECT 1 FROM core.land_contracts l WHERE l.contract_id = NEW.id) THEN
    RETURN NEW;  -- договор не на участок: у него свои условия
  END IF;

  SELECT count(*) INTO missing
  FROM refdata.land_covenants c
  WHERE NOT EXISTS (
    SELECT 1 FROM core.land_contract_covenants k
    WHERE k.contract_id = NEW.id AND k.code = c.code
  );

  IF missing > 0 THEN
    RAISE EXCEPTION
      'INV-105: в договоре на участок не закреплены особые условия (%), п. 107',
      missing
      USING ERRCODE = 'raise_exception';
  END IF;

  RETURN NEW;
END $$;

CREATE TRIGGER check_land_covenants BEFORE UPDATE ON core.contracts
  FOR EACH ROW EXECUTE FUNCTION core.check_land_covenants();
