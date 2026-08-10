-- Тендер, лоты, тендерная документация (М3).

CREATE TABLE core.tenders (
  id                  uuid               PRIMARY KEY DEFAULT uuidv7(),
  status              core.tender_status NOT NULL DEFAULT 'draft',
  title               text               NOT NULL,
  organizer_id        uuid               NOT NULL REFERENCES core.users (id),
  announced_at        timestamptz,  -- публикация объявления (FR-303, п. 5–6)
  submission_deadline timestamptz,  -- дедлайн приема заявок (п. 36–39, INV-037)
  opening_at          timestamptz,  -- назначенное время заседания/вскрытия (п. 50)
  opened_at           timestamptz,  -- факт вскрытия секретарем (FR-403): открывает цены (INV-040)
  trading_at          timestamptz,  -- дата/время торгов (п. 59, 62)
  zoom_url            text,         -- FR-306, ответ заказчика № 14
  zoom_recording_url  text,
  repeat_of           uuid          REFERENCES core.tenders (id),  -- повторный тендер (п. 82)
  created_at          timestamptz   NOT NULL DEFAULT now(),
  updated_at          timestamptz   NOT NULL DEFAULT now(),
  -- Прием заявок закрывается не позже вскрытия; вскрытие не раньше назначенного заседания (FR-403)
  CONSTRAINT deadline_before_opening
    CHECK (submission_deadline IS NULL OR opening_at IS NULL OR submission_deadline <= opening_at),
  CONSTRAINT opened_not_before_meeting
    CHECK (opened_at IS NULL OR opening_at IS NULL OR opened_at >= opening_at)
);

CREATE TRIGGER touch_updated_at BEFORE UPDATE ON core.tenders
  FOR EACH ROW EXECUTE FUNCTION core.touch_updated_at();

-- INV-021: переходы статусов — только из refdata.tender_status_transitions (FR-302).
-- Дополнительно при публикации: объявление ≥ 10 календарных дней до вскрытия (FR-303, п. 5).
CREATE FUNCTION core.enforce_tender_transition() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.status IS DISTINCT FROM NEW.status THEN
    IF NOT EXISTS (
      SELECT 1 FROM refdata.tender_status_transitions t
      WHERE t.from_status = OLD.status AND t.to_status = NEW.status
    ) THEN
      RAISE EXCEPTION 'INV-021: переход статуса тендера % -> % запрещен', OLD.status, NEW.status
        USING ERRCODE = 'check_violation';
    END IF;

    IF NEW.status IN ('announced', 'repeat_announced') THEN
      IF NEW.opening_at IS NULL OR NEW.submission_deadline IS NULL THEN
        RAISE EXCEPTION 'FR-303: публикация без дат вскрытия и дедлайна приема невозможна'
          USING ERRCODE = 'check_violation';
      END IF;
      IF NEW.opening_at < now() + interval '10 days' THEN
        RAISE EXCEPTION 'FR-303: между публикацией и вскрытием должно быть >= 10 календарных дней'
          USING ERRCODE = 'check_violation';
      END IF;
      NEW.announced_at := now();  -- время сервера — единственное юридически значимое (NFR-03)
    END IF;
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER enforce_tender_transition BEFORE UPDATE ON core.tenders
  FOR EACH ROW EXECUTE FUNCTION core.enforce_tender_transition();

-- Лот: снимок базовой ставки и расчета замораживается при создании (FR-202, FR-301)
CREATE TABLE core.lots (
  id                uuid          PRIMARY KEY DEFAULT uuidv7(),
  tender_id         uuid          NOT NULL REFERENCES core.tenders (id) ON DELETE CASCADE,
  seq               int           NOT NULL CHECK (seq > 0),
  object_id         uuid          NOT NULL REFERENCES core.objects (id),
  purpose           text          NOT NULL,  -- целевое назначение (Прил. 1 табл. 2)
  lease_months      int           NOT NULL CHECK (lease_months > 0),
  base_rate_monthly numeric(14,2) NOT NULL CHECK (base_rate_monthly > 0),  -- снимок FR-202
  rate_calculation  jsonb         NOT NULL,  -- полный RateCalculation: все множители (FR-201)
  guarantee_fee     numeric(14,2) NOT NULL CHECK (guarantee_fee > 0),  -- FR-206: = месячная ставка
  viewing_terms     text,         -- срок и условия осмотра
  UNIQUE (tender_id, seq)
);

-- Версии тендерной документации (FR-304): новая редакция — новая строка версии
CREATE TABLE core.tender_docs (
  id           uuid        PRIMARY KEY DEFAULT uuidv7(),
  tender_id    uuid        NOT NULL REFERENCES core.tenders (id) ON DELETE CASCADE,
  version      int         NOT NULL CHECK (version > 0),
  title        text        NOT NULL,
  file_key     text        NOT NULL,  -- RustFS
  published_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (tender_id, version, title)
);
