\set ON_ERROR_STOP on

-- Одноразовый, но повторно безопасный импорт лотов из Rent-lots.docx.
-- По умолчанию владельцем черновика становится демонстрационный организатор.
-- Для реальной учетной записи передайте:
--   psql ... -v organizer_email='organizer@tou.edu.kz' -f import-rent-lots.sql
\if :{?organizer_email}
\else
  \set organizer_email organizer@tou.demo
\endif

\if :{?tender_title}
\else
  \set tender_title 'Перечень лотов, выставляемых на тендер'
\endif

\if :{?tender_title_kk}
\else
  \set tender_title_kk 'Тендерге шығарылатын лоттар тізбесі'
\endif

BEGIN;

SELECT u.id AS actor_id
FROM core.users u
WHERE u.email = :'organizer_email'::citext
  AND u.is_active
  AND EXISTS (
    SELECT 1
    FROM core.role_grants rg
    WHERE rg.user_id = u.id
      AND rg.role IN ('organizer', 'admin')
  )
LIMIT 1
\gset

\if :{?actor_id}
\else
  \echo 'Активный organizer/admin с email ' :organizer_email ' не найден.'
  \quit 3
\endif

SELECT set_config('app.user_id', :'actor_id', true);

CREATE TEMP TABLE imported_rent_lots (
  seq         integer PRIMARY KEY,
  name_ru     text NOT NULL,
  name_kk     text NOT NULL,
  address_ru  text NOT NULL,
  address_kk  text NOT NULL,
  area_m2     numeric(10, 2) NOT NULL,
  purpose_ru  text NOT NULL,
  purpose_kk  text NOT NULL,
  price       numeric(14, 2) NOT NULL
) ON COMMIT DROP;

INSERT INTO imported_rent_lots
  (seq, name_ru, name_kk, address_ru, address_kk, area_m2, purpose_ru, purpose_kk, price)
VALUES
  (
    1,
    'Помещение буфета в здании Кампуса 1',
    '1-ші Кампус ғимаратындағы буфет үй-жайы',
    'г. Павлодар, ул. Толстого 101, 1 этаж',
    'Павлодар қ., Толстой көшесі 101, 1-ші қабат',
    43.70,
    'Организация общественного питания (буфет)',
    'Қоғамдық тамақтандыруды (буфет) ұйымдастыру',
    63439.00
  ),
  (
    2,
    'Помещение буфета в здании Кампуса 2',
    '2-ші Кампус ғимаратындағы буфет үй-жайы',
    'г. Павлодар, ул. Академика Чокина 139/1, 2 этаж',
    'Павлодар қ., Академик Шоқин көшесі 139/1, 2-ші қабат',
    66.40,
    'Организация общественного питания (буфет)',
    'Қоғамдық тамақтандыруды (буфет) ұйымдастыру',
    95954.00
  ),
  (
    3,
    'Помещение буфета в здании Кампуса 3',
    '3-ші Кампус ғимаратындағы буфет үй-жайы',
    'г. Павлодар, ул. Ломова 64/2, 2 этаж',
    'Павлодар қ., Ломов көшесі 64/2, 2-ші қабат',
    28.11,
    'Организация общественного питания (буфет)',
    'Қоғамдық тамақтандыруды (буфет) ұйымдастыру',
    40621.00
  ),
  (
    4,
    'Помещение буфета в учебном корпусе А',
    'А оқу корпусындағы буфет үй-жайы',
    'г. Павлодар, ул. Ломова 64, 2 этаж',
    'Павлодар қ., Ломов көшесі 64, 2-ші қабат',
    31.20,
    'Организация общественного питания (буфет)',
    'Қоғамдық тамақтандыруды (буфет) ұйымдастыру',
    55663.00
  ),
  (
    5,
    'Помещение буфета в учебном корпусе Д (колледж)',
    'Д оқу корпусындағы (колледж) буфет үй-жайы',
    'г. Павлодар, ул. Толстого 101А',
    'Павлодар қ., Толстой көшесі 101А',
    17.90,
    'Организация общественного питания (буфет)',
    'Қоғамдық тамақтандыруды (буфет) ұйымдастыру',
    38322.00
  ),
  (
    6,
    'Помещение (кабинет) стоматолога',
    'Стоматолог үй-жайы (кабинеті)',
    'г. Павлодар, ул. Академика Чокина 139/1, 1 этаж',
    'Павлодар қ., Академик Шоқин көшесі 139/1, 1-ші қабат',
    35.60,
    'Оказание медицинских / стоматологических услуг',
    'Медициналық / стоматологиялық қызметтер көрсету',
    102890.00
  ),
  (
    7,
    'Свободная площадь на 2 этаже главного учебного корпуса А (возле столовой)',
    'А бас оқу корпусының 2-ші қабатындағы бос алаң (асхананың жанында)',
    'г. Павлодар, ул. Ломова 64, 2 этаж',
    'Павлодар қ., Ломов көшесі 64, 2-ші қабат',
    9.00,
    'Размещение коммерческого объекта / кофеостровок',
    'Коммерциялық объектіні / кофеостровок орналастыру',
    53522.00
  ),
  (
    8,
    'Свободная площадь на 1 этаже в холле учебного корпуса Б (машфак)',
    'Б оқу корпусының (машфак) 1-ші қабатындағы холдағы бос алаң',
    'г. Павлодар, ул. Академика Чокина 139, 1 этаж',
    'Павлодар қ., Академик Шоқин көшесі 139, 1-ші қабат',
    9.00,
    'Размещение коммерческого объекта / кофеостровок',
    'Коммерциялық объектіні / кофеостровок орналастыру',
    32113.00
  );

INSERT INTO core.objects (kind, name, name_kk, address, address_kk, area_m2)
SELECT 'premises', i.name_ru, i.name_kk, i.address_ru, i.address_kk, i.area_m2
FROM imported_rent_lots i
WHERE NOT EXISTS (
  SELECT 1
  FROM core.objects o
  WHERE o.name = i.name_ru
    AND o.address = i.address_ru
    AND o.area_m2 = i.area_m2
);

INSERT INTO core.tenders (title, title_kk, organizer_id)
SELECT :'tender_title', :'tender_title_kk', :'actor_id'::uuid
WHERE NOT EXISTS (
  SELECT 1
  FROM core.tenders t
  WHERE t.title = :'tender_title'
    AND t.organizer_id = :'actor_id'::uuid
);

UPDATE core.tenders
SET title_kk = :'tender_title_kk'
WHERE title = :'tender_title'
  AND organizer_id = :'actor_id'::uuid
  AND title_kk IS DISTINCT FROM :'tender_title_kk';

SELECT t.id AS tender_id
FROM core.tenders t
WHERE t.title = :'tender_title'
  AND t.organizer_id = :'actor_id'::uuid
ORDER BY t.created_at
LIMIT 1
\gset

INSERT INTO core.lots (
  tender_id,
  seq,
  object_id,
  purpose,
  purpose_kk,
  lease_months,
  base_rate_monthly,
  guarantee_fee,
  rate_calculation,
  rate_unit
)
SELECT
  :'tender_id'::uuid,
  i.seq,
  o.id,
  i.purpose_ru,
  i.purpose_kk,
  12,
  i.price,
  i.price,
  jsonb_build_object(
    'source', 'Rent-lots.docx',
    'method', 'approved_starting_price',
    'amount', i.price,
    'currency', 'KZT',
    'rate_unit', 'monthly'
  ),
  'monthly'
FROM imported_rent_lots i
JOIN LATERAL (
  SELECT existing.id
  FROM core.objects existing
  WHERE existing.name = i.name_ru
    AND existing.address = i.address_ru
    AND existing.area_m2 = i.area_m2
  ORDER BY existing.created_at
  LIMIT 1
) o ON true
WHERE NOT EXISTS (
  SELECT 1
  FROM core.lots l
  WHERE l.tender_id = :'tender_id'::uuid
    AND l.seq = i.seq
);

COMMIT;

SELECT
  t.id AS tender_id,
  t.status,
  t.title,
  count(l.id) AS lots
FROM core.tenders t
JOIN core.lots l ON l.tender_id = t.id
WHERE t.id = :'tender_id'::uuid
GROUP BY t.id, t.status, t.title;
