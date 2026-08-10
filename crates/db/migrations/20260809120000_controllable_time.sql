-- Управляемое время стенда (T68, ADR-0005).
--
-- Сквозные сценарии не могут пройти путь «объявление -> договор» целиком:
-- FR-303 требует не менее десяти календарных дней между публикацией и
-- вскрытием, а прогон длится минуты. Поэтому приемка стартовала с середины,
-- вставляя готовое состояние, и стыки между шагами не проверялись.
--
-- Сдвинуть время в приложении нельзя: все юридически значимые отметки
-- ставит БД (NFR-03). Значит, единственный источник времени для правил
-- и отметок становится явным - `core.now()`.
--
-- Возможность сдвинуть время - это возможность подделать юридически
-- значимую отметку, поэтому она закрыта тремя рубежами (ADR-0005):
-- права роли приложения, явного намерения и следа в аудите.

CREATE TABLE refdata.clock_offset (
  -- Строка ровно одна: у стенда одни часы
  id    boolean     PRIMARY KEY DEFAULT true CHECK (id),
  shift interval    NOT NULL DEFAULT '0',
  -- Кто и когда сдвинул: сама отметка берется из системных часов, иначе
  -- запись о сдвиге двигалась бы вместе со сдвигом
  set_at    timestamptz NOT NULL DEFAULT clock_timestamp(),
  set_by    text
);
COMMENT ON TABLE refdata.clock_offset IS
  'Сдвиг часов стенда (T68, ADR-0005); на проде остается нулевым';

INSERT INTO refdata.clock_offset (id, shift) VALUES (true, '0');

-- Рубеж 1 (право): обычный путь запроса сдвинуть время не может в принципе.
-- Это не проверка, которую можно забыть, а отсутствие привилегии.
REVOKE INSERT, UPDATE, DELETE ON refdata.clock_offset FROM tou_rent_app;

-- Рубеж 3 (след): сдвиг часов - мутация домена и попадает в audit.log
-- (FR-1601). Ключ у таблицы естественный, поэтому вариант для естественных
-- ключей, как у справочников (T53).
CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON refdata.clock_offset
  FOR EACH ROW EXECUTE FUNCTION audit.record_natural_key();

-- Единственный источник времени для правил и отметок. STABLE, а не
-- IMMUTABLE: значение постоянно внутри запроса, но не между запросами.
CREATE FUNCTION core.now() RETURNS timestamptz
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, refdata AS $$
  SELECT now() + coalesce((SELECT shift FROM refdata.clock_offset WHERE id), '0');
$$;
COMMENT ON FUNCTION core.now() IS
  'Время сервера с учетом сдвига стенда (T68, ADR-0005); на проде равно now()';

-- --- Перевод правил на core.now() ------------------------------------------
--
-- Переопределяются те функции, которые сравнивают сроки или проставляют
-- отметки: половина правил в одних часах, половина в других - хуже, чем
-- отсутствие сдвига вовсе.

-- FR-303: окно в десять календарных дней между публикацией и вскрытием
CREATE OR REPLACE FUNCTION core.enforce_tender_transition() RETURNS trigger
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
      IF NOT EXISTS (SELECT 1 FROM core.lots l WHERE l.tender_id = NEW.id) THEN
        RAISE EXCEPTION 'FR-303: публикация тендера без хотя бы одного лота невозможна'
          USING ERRCODE = 'check_violation';
      END IF;
      IF NEW.opening_at IS NULL OR NEW.submission_deadline IS NULL THEN
        RAISE EXCEPTION 'FR-303: публикация без дат вскрытия и дедлайна приема невозможна'
          USING ERRCODE = 'check_violation';
      END IF;
      IF NEW.opening_at < core.now() + interval '10 days' THEN
        RAISE EXCEPTION 'FR-303: между публикацией и вскрытием должно быть >= 10 календарных дней'
          USING ERRCODE = 'check_violation';
      END IF;
      NEW.announced_at := core.now();
    END IF;
  END IF;
  RETURN NEW;
END $$;

-- INV-037: прием заявок закрывается в дедлайн
CREATE OR REPLACE FUNCTION core.journal_before_insert() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  deadline timestamptz;
BEGIN
  SELECT submission_deadline INTO deadline FROM core.tenders WHERE id = NEW.tender_id;

  IF deadline IS NOT NULL AND core.now() > deadline THEN
    RAISE EXCEPTION 'INV-037: прием закрыт - дедлайн % истек (п. 37–39)', deadline
      USING ERRCODE = 'check_violation';
  END IF;

  -- Сервер - единственный источник времени и порядка (NFR-03)
  NEW.occurred_at := core.now();
  INSERT INTO core.journal_counters AS c (tender_id, last_seq)
  VALUES (NEW.tender_id, 1)
  ON CONFLICT (tender_id) DO UPDATE SET last_seq = c.last_seq + 1
  RETURNING last_seq INTO NEW.seq;

  RETURN NEW;
END $$;

-- INV-066: время торгов истекает по тем же часам, что и назначается
CREATE OR REPLACE FUNCTION core.enforce_bid_rules() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  a           core.auctions%ROWTYPE;
  current_max numeric(14,2);
  circle      core.auction_participants%ROWTYPE;
BEGIN
  SELECT * INTO a FROM core.auctions WHERE id = NEW.auction_id FOR UPDATE;

  -- Оглашение первоначального предложения (п. 70) идет при старте торгов
  -- и правилам круга не подчиняется: это не ставка участника
  IF NEW.announced THEN
    IF a.status NOT IN ('scheduled', 'running') THEN
      RAISE EXCEPTION 'FR-605: огласить предложение можно только до конца торгов (статус %)', a.status
        USING ERRCODE = 'check_violation';
    END IF;
    NEW.placed_at := core.now();
    RETURN NEW;
  END IF;

  IF a.status <> 'running' THEN
    RAISE EXCEPTION 'ставка отклонена: аукцион не в статусе running (текущий: %)', a.status
      USING ERRCODE = 'check_violation';
  END IF;
  IF a.ends_at IS NOT NULL AND core.now() > a.ends_at THEN
    RAISE EXCEPTION 'INV-066: время торгов истекло (%)', a.ends_at
      USING ERRCODE = 'check_violation';
  END IF;

  -- Очередность по кругу (FR-604, п. 65): ходит тот, чья очередь
  SELECT * INTO circle FROM core.auction_participants
  WHERE auction_id = NEW.auction_id AND application_id = NEW.application_id;

  IF circle.id IS NOT NULL THEN
    IF circle.status = 'passed' THEN
      RAISE EXCEPTION 'FR-604: участник выбыл из торгов и больше не повышает (п. 65)'
        USING ERRCODE = 'check_violation';
    END IF;
    IF circle.status = 'absent' THEN
      RAISE EXCEPTION 'FR-605: участник не явился - объявлено его первоначальное предложение (п. 70)'
        USING ERRCODE = 'check_violation';
    END IF;
    IF a.current_turn_application_id IS NOT NULL
       AND a.current_turn_application_id <> NEW.application_id THEN
      RAISE EXCEPTION 'FR-604: сейчас ход другого участника (п. 65)'
        USING ERRCODE = 'check_violation';
    END IF;
  END IF;

  -- Оглашенные предложения (п. 70) ниже стартовой ставки: планка не может
  -- опуститься ниже старта, поэтому берется наибольшее из двух (INV-062–063)
  SELECT greatest(max(amount), a.starting_bid) INTO current_max
  FROM core.bids WHERE auction_id = NEW.auction_id;
  current_max := coalesce(current_max, a.starting_bid);

  IF NEW.amount < current_max + a.bid_step THEN
    RAISE EXCEPTION 'INV-063: ставка % ниже минимально допустимой % (максимум % + шаг %)',
      NEW.amount, current_max + a.bid_step, current_max, a.bid_step
      USING ERRCODE = 'check_violation';
  END IF;

  NEW.placed_at := core.now();  -- время сервера (NFR-03)
  RETURN NEW;
END $$;
