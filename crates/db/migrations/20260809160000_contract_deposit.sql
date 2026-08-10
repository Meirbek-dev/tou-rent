-- Депозит по договору (T74, FR-1003, п. 132–136).
--
-- Механика книги для депозита уже была (двойная запись, операции списания
-- и восполнения), не хватало одного: основание возврата.
--
-- CHECK `refund_needs_reason` требовал основание у любой операции `refund`,
-- но закрытый перечень п. 26 - это основания возврата **гарантийного
-- взноса участника**. К депозиту по договору они неприменимы: его возврат
-- предусмотрен самим п. 136 и наступает от факта возврата объекта, а не от
-- выбора оператора. С прежним CHECK'ом возврат депозита был невозможен
-- вовсе - оператору пришлось бы указать чужое основание.
--
-- Правило разделено по типу счета, а раз проверка смотрит в другую таблицу,
-- она становится триггером: CHECK подзапросов не допускает.

ALTER TABLE core.ledger_entries DROP CONSTRAINT IF EXISTS refund_needs_reason;

CREATE OR REPLACE FUNCTION core.check_refund_reason() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  account_kind core.ledger_account_kind;
BEGIN
  SELECT kind INTO account_kind
    FROM core.ledger_accounts WHERE id = NEW.account_id;

  IF NEW.op = 'refund' THEN
    IF account_kind = 'participant_fee' AND NEW.refund_reason IS NULL THEN
      RAISE EXCEPTION
        'FR-1002: возврат гарантийного взноса требует основания из перечня п. 26'
        USING ERRCODE = 'check_violation';
    END IF;
    -- Депозит возвращается по п. 136: основание - сам факт возврата объекта,
    -- и перечень п. 26 к нему не относится
    IF account_kind = 'contract_deposit' AND NEW.refund_reason IS NOT NULL THEN
      RAISE EXCEPTION
        'FR-1003: у возврата депозита нет основания из перечня п. 26 (п. 136)'
        USING ERRCODE = 'check_violation';
    END IF;
  ELSIF NEW.refund_reason IS NOT NULL THEN
    RAISE EXCEPTION
      'FR-1002: основание возврата заполняется только у операции возврата'
      USING ERRCODE = 'check_violation';
  END IF;

  RETURN NEW;
END $$;

COMMENT ON FUNCTION core.check_refund_reason() IS
  'FR-1002, FR-1003: основание п. 26 - только у возврата взноса участника';

DROP TRIGGER IF EXISTS check_refund_reason ON core.ledger_entries;
CREATE TRIGGER check_refund_reason BEFORE INSERT ON core.ledger_entries
  FOR EACH ROW EXECUTE FUNCTION core.check_refund_reason();

-- Ранее заключенные договоры (T74): счет депозита открывается при
-- регистрации, но договоры, зарегистрированные до этой миграции, ее не
-- проходили - без переноса FR-1003 действовал бы только для новых.
--
-- Открываются счета, но не ставятся сроки: срок п. 132 отсчитывается от
-- заключения договора, и для старых он либо давно прошел, либо неизвестен.
-- Проставить его задним числом значило бы выдумать дату юридически
-- значимого срока; для действующих договоров это решение инженера.
INSERT INTO core.ledger_accounts (kind, contract_id, owner_user_id)
SELECT 'contract_deposit', c.id, c.tenant_id
  FROM core.contracts c
 WHERE c.registered_at IS NOT NULL
   AND NOT EXISTS (SELECT 1 FROM core.ledger_accounts a
                    WHERE a.kind = 'contract_deposit' AND a.contract_id = c.id);
