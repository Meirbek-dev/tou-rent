-- Основания возврата гарантийного взноса (М10, FR-1002, п. 26): закрытый
-- перечень из шести случаев — тот же прием, что и у оснований отклонения
-- (INV-052): справочник + FK, паритет с enum домена проверяет тест.

CREATE TABLE refdata.refund_reasons (
  code     text PRIMARY KEY,
  label_ru text NOT NULL,
  label_kk text,
  label_en text,
  rule_ref text NOT NULL  -- подпункт п. 26
);

-- TODO-ENGINEER: формулировки и нумерация подпунктов п. 26 выверяются по
-- Правилам (Q-003). Состав перечня — из ТЗ FR-1002 («шесть случаев»),
-- тексты ниже черновые и подлежат сверке.
INSERT INTO refdata.refund_reasons (code, label_ru, label_kk, label_en, rule_ref) VALUES
  ('application_withdrawn', 'Заявка отозвана до окончания срока приема',
   'Өтінім қабылдау мерзімі аяқталғанға дейін кері қайтарып алынды',
   'Application withdrawn before the deadline', 'п. 26.1'),
  ('tender_failed', 'Тендер признан несостоявшимся либо отменен',
   'Тендер өткізілмеді деп танылды немесе жойылды',
   'Tender declared failed or cancelled', 'п. 26.2'),
  ('not_admitted', 'Участник не допущен по итогам первого этапа',
   'Қатысушы бірінші кезең қорытындысы бойынша жіберілмеді',
   'Participant not qualified at the first stage', 'п. 26.3'),
  ('not_winner', 'Участник не признан победителем и не занял второе место',
   'Қатысушы жеңімпаз болмады және екінші орын алмады',
   'Participant is neither the winner nor the runner-up', 'п. 26.4'),
  ('terms_changed', 'Условия тендера изменены, участник отказался от участия',
   'Тендер шарттары өзгерді, қатысушы қатысудан бас тартты',
   'Tender terms changed and the participant withdrew', 'п. 26.5'),
  ('contract_signed', 'Договор заключен: взнос возвращается либо засчитывается',
   'Шарт жасалды: жарна қайтарылады немесе есепке алынады',
   'Contract signed: the fee is returned or offset', 'п. 26.6')
ON CONFLICT DO NOTHING;

-- Основание проводки-возврата — только из перечня (FR-1002): текстовое
-- поле rule_ref остается для пояснения, а код основания проверяется FK
ALTER TABLE core.ledger_entries
  ADD COLUMN refund_reason text REFERENCES refdata.refund_reasons (code);

COMMENT ON COLUMN core.ledger_entries.refund_reason IS
  'FR-1002: основание возврата из закрытого перечня п. 26; заполняется только для op = refund';

ALTER TABLE core.ledger_entries
  ADD CONSTRAINT refund_needs_reason
  CHECK ((op = 'refund') = (refund_reason IS NOT NULL));

-- Поступление взноса подтверждается не позднее чем за 2 рабочих дня до
-- первого этапа (п. 23): дату поступления вводит оператор финблока,
-- поэтому проверка живет в БД, а не только в форме.
ALTER TABLE core.ledger_entries
  ADD COLUMN paid_at date;

COMMENT ON COLUMN core.ledger_entries.paid_at IS
  'Дата фактического поступления денег (FR-405): вводится оператором, банк-интеграции нет';

ALTER TABLE core.ledger_entries
  ADD CONSTRAINT receipt_needs_paid_at
  CHECK ((op = 'receipt_confirmed') = (paid_at IS NOT NULL));
