-- Тендерная комиссия целиком (М11, FR-1101–1104): утверждение состава,
-- явка и кворум заседания, личное голосование членов, конфликт интересов
-- и отвод. Правила из п. 9–15 закрепляются здесь — последний рубеж (арх. § 3);
-- те же правила выражены типами в `crates/domain/src/commission.rs`.

-- ---------------------------------------------------------------- состав ---

-- Срок полномочий комиссии — один год (п. 9)
ALTER TABLE core.commissions
  ADD CONSTRAINT term_at_most_one_year
  CHECK (valid_until <= valid_from + interval '1 year');

-- Состав утверждается целиком: до утверждения он собирается по одному
-- человеку и проверять его построчно бессмысленно (FR-1101)
ALTER TABLE core.commissions
  ADD COLUMN approved_at timestamptz,
  ADD COLUMN approved_by uuid REFERENCES core.users (id);

COMMENT ON COLUMN core.commissions.approved_at IS
  'Состав утвержден и проверен (п. 9–11); заседания ведет только утвержденная комиссия';

-- Резервный член голоса не имеет, пока не заменил отведенного (п. 15)
CREATE FUNCTION core.commission_voting_count(p_commission uuid) RETURNS integer
LANGUAGE sql STABLE AS $$
  SELECT count(*)::integer FROM core.commission_members
  WHERE commission_id = p_commission AND member_role <> 'reserve'
$$;

-- Проверка состава (п. 9–11): один председатель, один заместитель,
-- голосующих нечетное число и не меньше семи. Секретарь в состав не входит
-- (п. 16–17) — пользователь с ролью secretary членом быть не может.
CREATE FUNCTION core.assert_commission_composition(p_commission uuid) RETURNS void
LANGUAGE plpgsql STABLE AS $$
DECLARE
  chairmen integer;
  deputies integer;
  voting   integer;
  clerk    integer;
BEGIN
  SELECT
    count(*) FILTER (WHERE member_role = 'chairman'),
    count(*) FILTER (WHERE member_role = 'deputy'),
    count(*) FILTER (WHERE member_role <> 'reserve')
  INTO chairmen, deputies, voting
  FROM core.commission_members WHERE commission_id = p_commission;

  IF chairmen <> 1 THEN
    RAISE EXCEPTION 'FR-1101: председатель должен быть ровно один (сейчас %)', chairmen;
  END IF;
  IF deputies <> 1 THEN
    RAISE EXCEPTION 'FR-1101: заместитель председателя должен быть ровно один (сейчас %)', deputies;
  END IF;
  IF voting < 7 THEN
    RAISE EXCEPTION 'FR-1101: голосующих членов %, требуется не менее 7 (п. 9)', voting;
  END IF;
  IF voting % 2 = 0 THEN
    RAISE EXCEPTION 'FR-1101: голосующих членов % — состав должен быть нечетным (п. 9)', voting;
  END IF;

  SELECT count(*) INTO clerk
  FROM core.commission_members cm
  JOIN core.role_grants rg ON rg.user_id = cm.user_id AND rg.role = 'secretary'
  WHERE cm.commission_id = p_commission;
  IF clerk > 0 THEN
    RAISE EXCEPTION 'FR-1101: секретарь комиссии в ее состав не входит и не голосует (п. 16–17)';
  END IF;
END $$;

CREATE FUNCTION core.check_commission_approval() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.approved_at IS NOT NULL AND OLD.approved_at IS DISTINCT FROM NEW.approved_at THEN
    PERFORM core.assert_commission_composition(NEW.id);
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER check_composition BEFORE UPDATE ON core.commissions
  FOR EACH ROW EXECUTE FUNCTION core.check_commission_approval();

-- Правка состава утвержденной комиссии сбрасывает утверждение: измененный
-- состав обязан пройти проверку заново
CREATE FUNCTION core.reset_commission_approval() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  UPDATE core.commissions SET approved_at = NULL, approved_by = NULL
  WHERE id = COALESCE(NEW.commission_id, OLD.commission_id) AND approved_at IS NOT NULL;
  RETURN NULL;
END $$;

CREATE TRIGGER reset_approval AFTER INSERT OR UPDATE OR DELETE ON core.commission_members
  FOR EACH ROW EXECUTE FUNCTION core.reset_commission_approval();

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.commissions
  FOR EACH ROW EXECUTE FUNCTION audit.record();
CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.commission_members
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- --------------------------------------------- конфликт интересов и отвод ---

-- Отвод члена комиссии (FR-1104, п. 15): решение большинства фиксирует
-- секретарь; отведенного заменяет резервный, материалы лота ему закрыты.
CREATE TABLE core.member_recusals (
  id                    uuid        PRIMARY KEY DEFAULT uuidv7(),
  tender_id             uuid        NOT NULL REFERENCES core.tenders (id),
  member_id             uuid        NOT NULL REFERENCES core.commission_members (id),
  -- NULL — отвод по всему тендеру; иначе только по этому лоту
  lot_id                uuid        REFERENCES core.lots (id),
  reason                text        NOT NULL,
  replacement_member_id uuid        REFERENCES core.commission_members (id),
  decided_at            timestamptz NOT NULL DEFAULT now(),
  decided_by            uuid        REFERENCES core.users (id),
  UNIQUE (tender_id, member_id),
  CHECK (replacement_member_id IS DISTINCT FROM member_id)
);

CREATE INDEX member_recusals_tender_idx ON core.member_recusals (tender_id);

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.member_recusals
  FOR EACH ROW EXECUTE FUNCTION audit.record();
CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.coi_declarations
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Отведен ли член комиссии по заявке (лоту): используется голосованием,
-- RLS цен и выборками кабинета
CREATE FUNCTION core.member_recused(p_member uuid, p_tender uuid, p_lot uuid)
RETURNS boolean LANGUAGE sql STABLE AS $$
  SELECT EXISTS (
    SELECT 1 FROM core.member_recusals r
    WHERE r.member_id = p_member AND r.tender_id = p_tender
      AND (r.lot_id IS NULL OR r.lot_id = p_lot)
  )
$$;

-- Отведенный не видит материалы лота (FR-1104, п. 15): дополнение к
-- запечатанности цен INV-040 — политика пересоздается целиком, чтобы
-- условие читалось одним куском.
DROP POLICY sealed_until_opening ON core.price_proposals;

CREATE POLICY sealed_until_opening ON core.price_proposals FOR SELECT
  USING (
    EXISTS (
      SELECT 1
      FROM core.applications a
      JOIN core.tenders t ON t.id = a.tender_id
      WHERE a.id = price_proposals.application_id
        AND (t.opened_at IS NOT NULL                          -- вскрытие состоялось (FR-403)
             OR a.participant_id = core.current_app_user())   -- участник видит свое предложение
    )
    AND NOT EXISTS (                                          -- отведенному лот закрыт (FR-1104)
      SELECT 1
      FROM core.applications a
      JOIN core.commission_members cm ON cm.user_id = core.current_app_user()
      WHERE a.id = price_proposals.application_id
        AND core.member_recused(cm.id, a.tender_id, a.lot_id)
    )
  );

-- ----------------------------------------------------- заседание и кворум ---

ALTER TABLE core.sessions_meetings
  ADD COLUMN opened_at       timestamptz,
  ADD COLUMN quorum_present  integer,
  ADD COLUMN quorum_required integer;

COMMENT ON COLUMN core.sessions_meetings.opened_at IS
  'Заседание открыто при кворуме (п. 12); до этого решения комиссии невозможны';

-- Явка (п. 12): кто присутствует и кто председательствует
CREATE TABLE core.meeting_attendance (
  id          uuid        PRIMARY KEY DEFAULT uuidv7(),
  meeting_id  uuid        NOT NULL REFERENCES core.sessions_meetings (id) ON DELETE CASCADE,
  member_id   uuid        NOT NULL REFERENCES core.commission_members (id),
  present     boolean     NOT NULL DEFAULT true,
  -- Председательствующий: его голос решает при равенстве (п. 14)
  chairing    boolean     NOT NULL DEFAULT false,
  recorded_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (meeting_id, member_id),
  CHECK (present OR NOT chairing)
);

CREATE UNIQUE INDEX one_chairing_per_meeting
  ON core.meeting_attendance (meeting_id) WHERE chairing;

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.meeting_attendance
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Председательствовать может только председатель или его заместитель (п. 12),
-- и только член той комиссии, что ведет заседание
CREATE FUNCTION core.check_attendance() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  role_of core.commission_member_role;
  same    boolean;
BEGIN
  SELECT cm.member_role, cm.commission_id = m.commission_id
  INTO role_of, same
  FROM core.commission_members cm, core.sessions_meetings m
  WHERE cm.id = NEW.member_id AND m.id = NEW.meeting_id;

  IF NOT COALESCE(same, false) THEN
    RAISE EXCEPTION 'FR-1102: член комиссии не входит в состав, ведущий это заседание';
  END IF;
  IF NEW.chairing AND role_of NOT IN ('chairman', 'deputy') THEN
    RAISE EXCEPTION 'FR-1102: председательствует только председатель или его заместитель (п. 12)';
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER check_attendance BEFORE INSERT OR UPDATE ON core.meeting_attendance
  FOR EACH ROW EXECUTE FUNCTION core.check_attendance();

-- Кворум ⅔ голосующего состава с председателем или заместителем (п. 12):
-- заседание не открывается без него. Отведенные по тендеру в явке не
-- учитываются — их место занимает резервный (п. 15)
CREATE FUNCTION core.check_meeting_quorum() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  voting_total integer;
  present_cnt  integer;
  chair_here   boolean;
  required     integer;
BEGIN
  IF NEW.opened_at IS NULL OR OLD.opened_at IS NOT DISTINCT FROM NEW.opened_at THEN
    RETURN NEW;
  END IF;

  IF NOT EXISTS (SELECT 1 FROM core.commissions c
                 WHERE c.id = NEW.commission_id AND c.approved_at IS NOT NULL) THEN
    RAISE EXCEPTION 'FR-1101: состав комиссии не утвержден — заседание невозможно';
  END IF;

  voting_total := core.commission_voting_count(NEW.commission_id);

  SELECT
    count(*) FILTER (WHERE a.present),
    count(*) FILTER (WHERE a.present AND cm.member_role IN ('chairman', 'deputy')) > 0
  INTO present_cnt, chair_here
  FROM core.meeting_attendance a
  JOIN core.commission_members cm ON cm.id = a.member_id
  WHERE a.meeting_id = NEW.id AND cm.member_role <> 'reserve';

  required := ceil(voting_total * 2.0 / 3.0);

  IF COALESCE(present_cnt, 0) < required THEN
    RAISE EXCEPTION 'FR-1102: кворума нет — присутствует % из требуемых % (п. 12)',
      COALESCE(present_cnt, 0), required;
  END IF;
  IF NOT COALESCE(chair_here, false) THEN
    RAISE EXCEPTION 'FR-1102: нет ни председателя, ни его заместителя (п. 12)';
  END IF;

  NEW.quorum_present  := present_cnt;
  NEW.quorum_required := required;
  RETURN NEW;
END $$;

CREATE TRIGGER check_quorum BEFORE UPDATE ON core.sessions_meetings
  FOR EACH ROW EXECUTE FUNCTION core.check_meeting_quorum();

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.sessions_meetings
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Вскрытие проводится на открытом заседании комиссии (п. 12, 50):
-- без кворума конвертам не вскрываться
CREATE FUNCTION core.check_opening_meeting() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.opened_at IS NOT NULL AND OLD.opened_at IS NULL
     AND NOT EXISTS (
       SELECT 1 FROM core.sessions_meetings m
       WHERE m.tender_id = NEW.id AND m.kind = 'qualification' AND m.opened_at IS NOT NULL
     ) THEN
    RAISE EXCEPTION 'FR-1102: заседание комиссии не открыто — вскрытие невозможно (п. 12, 50)';
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER check_opening_meeting BEFORE UPDATE ON core.tenders
  FOR EACH ROW EXECUTE FUNCTION core.check_opening_meeting();

-- ------------------------------------------------------------ голосование ---

-- Голосует лично присутствующий член комиссии (FR-1103): не резервный (кроме
-- заменившего отведенного), не отведенный, на открытом заседании и по заявке
-- этого же тендера. «Воздержался» невозможен типом (INV-055).
CREATE FUNCTION core.check_vote() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  meeting     core.sessions_meetings;
  role_of     core.commission_member_role;
  is_present  boolean;
  app_tender  uuid;
  app_lot     uuid;
  replaces    boolean;
BEGIN
  SELECT * INTO meeting FROM core.sessions_meetings m WHERE m.id = NEW.meeting_id;
  IF meeting.opened_at IS NULL THEN
    RAISE EXCEPTION 'FR-1103: заседание не открыто — голосование невозможно (п. 12)';
  END IF;

  SELECT a.tender_id, a.lot_id INTO app_tender, app_lot
  FROM core.applications a WHERE a.id = NEW.application_id;
  IF app_tender IS DISTINCT FROM meeting.tender_id THEN
    RAISE EXCEPTION 'FR-1103: заявка относится к другому тендеру';
  END IF;

  SELECT cm.member_role INTO role_of
  FROM core.commission_members cm
  WHERE cm.id = NEW.member_id AND cm.commission_id = meeting.commission_id;
  IF role_of IS NULL THEN
    RAISE EXCEPTION 'FR-1103: голосующий не входит в состав комиссии заседания';
  END IF;

  IF core.member_recused(NEW.member_id, app_tender, app_lot) THEN
    RAISE EXCEPTION 'FR-1104: отведенный член комиссии не голосует по этому лоту (п. 15)';
  END IF;

  -- Резервный голосует, только заменив отведенного (п. 15)
  IF role_of = 'reserve' THEN
    SELECT EXISTS (
      SELECT 1 FROM core.member_recusals r
      WHERE r.replacement_member_id = NEW.member_id AND r.tender_id = app_tender
        AND (r.lot_id IS NULL OR r.lot_id = app_lot)
    ) INTO replaces;
    IF NOT COALESCE(replaces, false) THEN
      RAISE EXCEPTION 'FR-1103: резервный член комиссии голосует только вместо отведенного (п. 15)';
    END IF;
  END IF;

  SELECT a.present INTO is_present
  FROM core.meeting_attendance a
  WHERE a.meeting_id = NEW.meeting_id AND a.member_id = NEW.member_id;
  IF NOT COALESCE(is_present, false) THEN
    RAISE EXCEPTION 'FR-1103: голосует только присутствующий член комиссии (п. 13)';
  END IF;

  RETURN NEW;
END $$;

CREATE TRIGGER check_vote BEFORE INSERT OR UPDATE ON core.votes
  FOR EACH ROW EXECUTE FUNCTION core.check_vote();

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.votes
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Права роли приложения на новые таблицы (миграция 14 раздает их по умолчанию,
-- append-only вычеты — явно): голос переголосовать можно до решения по заявке,
-- удалить — нет.
REVOKE DELETE ON core.votes FROM tou_rent_app;
