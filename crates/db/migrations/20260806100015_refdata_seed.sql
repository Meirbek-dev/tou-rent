-- Seed справочников (T2). Идемпотентно (ON CONFLICT DO NOTHING): безопасно
-- и для чистой БД, и для прод-дампа. Предметные константы, отсутствующие в ТЗ,
-- агент не выдумывает (А.4): помечены TODO-ENGINEER с заведомо фиктивными значениями.

-- INV-021 / FR-302: разрешенные переходы статусной модели тендера
INSERT INTO refdata.tender_status_transitions (from_status, to_status) VALUES
  ('draft',            'announced'),        -- публикация объявления (FR-303)
  ('announced',        'accepting'),        -- открытие приема заявок (п. 36)
  ('accepting',        'qualification'),    -- дедлайн, вскрытие (п. 40, 50)
  ('qualification',    'trading'),          -- допуск завершен, торги назначены (п. 57–59)
  ('trading',          'summed_up'),        -- итоги торгов (п. 73)
  ('summed_up',        'contracted'),       -- договор заключен (п. 108)
  ('accepting',        'failed'),           -- 0 или 1 заявка (п. 81, FR-801)
  ('qualification',    'failed'),           -- допущено < 2 (п. 81)
  ('summed_up',        'failed'),           -- уклонение победителя и № 2 (п. 81)
  ('failed',           'repeat_announced'), -- повторный тендер (п. 82)
  ('repeat_announced', 'accepting'),
  ('draft',            'cancelled'),        -- отмена до заключения договора (FR-305, п. 78–79)
  ('announced',        'cancelled'),
  ('accepting',        'cancelled'),
  ('qualification',    'cancelled'),
  ('trading',          'cancelled'),
  ('summed_up',        'cancelled'),
  ('repeat_announced', 'cancelled')
ON CONFLICT DO NOTHING;

-- INV-052 / FR-502: закрытый перечень оснований отклонения (п. 52), kk/en — draft (A-004)
INSERT INTO refdata.rejection_reasons (code, label_ru, label_kk, label_en, rule_ref) VALUES
  ('non_compliant', 'Несоответствие требованиям тендерной документации',
   'Тендерлік құжаттама талаптарына сәйкес келмеуі', 'Non-compliance with tender documentation', 'п. 52.1'),
  ('fee_not_paid', 'Невнесение либо неполное внесение гарантийного взноса',
   'Кепілдік жарнаның енгізілмеуі немесе толық енгізілмеуі', 'Guarantee fee not paid or paid partially', 'п. 52.2'),
  ('affiliated', 'Аффилированность с другим участником по тому же лоту',
   'Сол лот бойынша басқа қатысушымен үлестестік', 'Affiliation with another bidder for the same lot', 'п. 52.3'),
  ('evader', 'Уклонение от подписания договора в прошлых тендерах',
   'Өткен тендерлерде шарт жасасудан жалтару', 'Evaded contract signing in previous tenders', 'п. 52.4, 120')
ON CONFLICT DO NOTHING;

-- FR-201: МРП 2026 — TODO-ENGINEER: подтвердить сумму (значение заведомо фиктивное)
INSERT INTO refdata.mrp (year, amount) VALUES (2026, 9999.00)
ON CONFLICT DO NOTHING;

-- Производственный календарь РК 2026 (G12). Национальные и государственные праздники;
-- TODO-ENGINEER: сверить переносы выходных дней с постановлением Правительства РК на 2026 год.
INSERT INTO refdata.holidays (day, label_ru) VALUES
  ('2026-01-01', 'Новый год'),
  ('2026-01-02', 'Новый год'),
  ('2026-01-07', 'Рождество Христово (православное)'),
  ('2026-03-08', 'Международный женский день'),
  ('2026-03-21', 'Наурыз мейрамы'),
  ('2026-03-22', 'Наурыз мейрамы'),
  ('2026-03-23', 'Наурыз мейрамы'),
  ('2026-05-01', 'Праздник единства народа Казахстана'),
  ('2026-05-07', 'День защитника Отечества'),
  ('2026-05-09', 'День Победы'),
  ('2026-07-06', 'День столицы'),
  ('2026-08-30', 'День Конституции'),
  ('2026-10-25', 'День Республики'),
  ('2026-12-16', 'День независимости')
ON CONFLICT DO NOTHING;

-- FR-202: каркас коэффициентов Прил. 4. TODO-ENGINEER: реальные опции и значения
-- из Правил (первоисточник-PDF агенту недоступен); value=1.0000 — заведомо нейтральный
-- плейсхолдер, кроме Ксоц=0.5 — зафиксировано в ТЗ (FR-1205).
INSERT INTO refdata.rate_coefficients
  (coefficient, option_code, label_ru, label_kk, label_en, value, effective_from)
VALUES
  ('kt',    'default', 'Тип помещения — базовый (TODO-ENGINEER)',        NULL, NULL, 1.0000, '2026-01-01'),
  ('kk',    'default', 'Комфортность — базовая (TODO-ENGINEER)',         NULL, NULL, 1.0000, '2026-01-01'),
  ('ksk',   'default', 'Кск — базовый (TODO-ENGINEER)',                  NULL, NULL, 1.0000, '2026-01-01'),
  ('kr',    'default', 'Расположение — базовое (TODO-ENGINEER)',         NULL, NULL, 1.0000, '2026-01-01'),
  ('kvd',   'default', 'Вид деятельности — базовый (TODO-ENGINEER)',     NULL, NULL, 1.0000, '2026-01-01'),
  ('kopf',  'default', 'Орг.-правовая форма — базовая (TODO-ENGINEER)',  NULL, NULL, 1.0000, '2026-01-01'),
  ('kfu',   'default', 'Кфу — базовый (TODO-ENGINEER)',                  NULL, NULL, 1.0000, '2026-01-01'),
  ('ksots', 'default', 'Социальный коэффициент — базовый',               NULL, NULL, 1.0000, '2026-01-01'),
  ('ksots', 'social',  'Социальный арендатор (FR-1205, п. 95–96)',       NULL, NULL, 0.5000, '2026-01-01'),
  ('k',     'default', 'К — базовый (TODO-ENGINEER)',                    NULL, NULL, 1.0000, '2026-01-01'),
  ('kn',    'default', 'Кн — базовый (TODO-ENGINEER)',                   NULL, NULL, 1.0000, '2026-01-01'),
  ('kv',    'default', 'Кв — базовый (TODO-ENGINEER)',                   NULL, NULL, 1.0000, '2026-01-01')
ON CONFLICT DO NOTHING;
