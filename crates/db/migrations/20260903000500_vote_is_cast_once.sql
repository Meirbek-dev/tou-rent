-- Голос члена комиссии подается один раз (FR-1103, п. 13–14).
--
-- Круг 4 гаунтлета (G-111). Голос писался `ON CONFLICT ... DO UPDATE`:
--
--   POST /api/v1/applications/{id}/vote {"value":"for"}     -> 200, «за»
--   POST /api/v1/applications/{id}/vote {"value":"against"} -> 200, «против»
--
-- Первый голос исчезал без следа в подсчете, хотя окно подтверждения
-- обещает обратное: «голос не может быть изменен». Между тем голос -
-- процедурный акт заседания: он оглашается, входит в подсчет и в протокол
-- допуска (п. 55), и переписывать его задним числом нельзя ни членам
-- комиссии, ни коду. Особое мнение (п. 13–14) - часть того же акта:
-- приложение к протоколу подменять так же нельзя.
--
-- Рубеж - таблица, а не обработчик: путей записи в `core.votes` может стать
-- больше, а правило одно. Обработчик, узнавший о конфликте, отвечает 409
-- с причиной `vote_already_cast` - до триггера он в норме не доходит.
--
-- Чего здесь намеренно НЕТ: запрета на удаление голоса. DELETE у роли
-- приложения отобран еще миграцией 20260807140000, а исправление ошибочно
-- внесенного состава заседания - работа администратора БД, а не правило
-- этого триггера.
--
-- Сообщение начинается идентификатором правила: по нему
-- `RuleViolation::from_message` (crates/domain/src/rule.rs) выбирает
-- причину перечня, и наружу уходит она, а не русский текст RAISE (NFR-07).

CREATE OR REPLACE FUNCTION core.check_vote_is_cast_once() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.value IS DISTINCT FROM OLD.value
     OR NEW.dissent IS DISTINCT FROM OLD.dissent THEN
    RAISE EXCEPTION
      'INV-1103: голос комиссии не переписывается - подан % (п. 13–14)', OLD.value
      USING ERRCODE = 'check_violation';
  END IF;
  RETURN NEW;
END $$;

COMMENT ON FUNCTION core.check_vote_is_cast_once() IS
  'INV-1103: поданный голос и особое мнение не изменяются (FR-1103, п. 13–14)';

DROP TRIGGER IF EXISTS check_vote_is_cast_once ON core.votes;
CREATE TRIGGER check_vote_is_cast_once BEFORE UPDATE ON core.votes
  FOR EACH ROW EXECUTE FUNCTION core.check_vote_is_cast_once();
ALTER TABLE core.votes ENABLE ALWAYS TRIGGER check_vote_is_cast_once;
