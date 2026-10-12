\set ON_ERROR_STOP on

-- Черновик из Объявление.docx от 12.10.2026. Публикация выполняется отдельно.
-- Запуск: psql ... -v organizer_email='реальный-email' -f этот-файл.sql
-- Постоянный ID служит ключом импорта; повторный запуск не меняет тендер.
\if :{?organizer_email}
\else
  \echo 'Передайте organizer_email активного организатора или администратора.'
  \quit 3
\endif

BEGIN;
SELECT pg_advisory_xact_lock(hashtextextended('announcement-20261012',0));

SELECT u.id AS actor_id
FROM core.users u
WHERE u.email=:'organizer_email'::citext AND u.is_active
  AND EXISTS (
    SELECT 1 FROM core.role_grants r
    WHERE r.user_id=u.id AND r.role IN ('organizer','admin')
  )
\gset

\if :{?actor_id}
\else
  \echo 'Активный organizer/admin с указанным email не найден; импорт отменен.'
  ROLLBACK;
  \quit 3
\endif

SELECT set_config('app.user_id',:'actor_id',true);
\set tender_id '597f17a0-a579-4f46-a5ab-65f1bb97cf20'

SELECT EXISTS(SELECT 1 FROM core.tenders WHERE id=:'tender_id'::uuid) AS already_imported
\gset
\if :already_imported
  \echo 'Тендер уже импортирован. Повторный запуск ничего не меняет.'
\else

CREATE TEMP TABLE announcement_lots (
  seq integer PRIMARY KEY,
  name_ru text NOT NULL,
  name_kk text NOT NULL,
  address_ru text NOT NULL,
  address_kk text NOT NULL,
  area_m2 numeric(10,2) NOT NULL,
  purpose_ru text NOT NULL,
  purpose_kk text NOT NULL,
  price numeric(14,2) NOT NULL,
  source_kind text NOT NULL,
  qualification_requirements text
) ON COMMIT DROP;

INSERT INTO announcement_lots VALUES
  (1,
   'Помещение буфета в здании Кампуса 1',
   '1-ші Кампус ғимаратындағы буфет үй-жайы',
   'г. Павлодар, ул. Толстого 101, 1 этаж',
   'Павлодар қ., Толстой көшесі 101, 1-ші қабат',
   43.70,
   'Организация общественного питания (буфет)',
   'Қоғамдық тамақтандыруды (буфет) ұйымдастыру',
   31720.00,'repeat',NULL),
  (8,
   'Свободная площадь на 1 этаже в холле учебного корпуса Б',
   'Б оқу корпусының 1-ші қабатындағы холдағы бос алаң',
   'г. Павлодар, ул. Академика Чокина 139, 1 этаж',
   'Павлодар қ., Академик Шоқин көшесі 139, 1-ші қабат',
   9.00,
   'Размещение коммерческого объекта / кофеостровок',
   'Коммерциялық объектіні / кофеостровок орналастыру',
   32113.00,'repeat',NULL),
  (11,
   'Нежилые помещения на 1 этаже здания студенческого общежития',
   'Студенттік жатақхана ғимаратының 1-ші қабатындағы тұрғын емес үй-жайлар',
   'г. Павлодар, ул. Академика Чокина, 139/1',
   'Павлодар қ., Академик Шоқин көшесі 139/1',
   658.20,
   'Организация деятельности в сфере здравоохранения (Медицинский центр)',
   'Денсаулық сақтау саласындағы қызметті ұйымдастыру (Медициналық орталық)',
   1409124.00,'primary',
   'Действующая государственная лицензия на медицинскую деятельность по соответствующим подвидам услуг. Сведения о квалификации по Приложению №11 с подтверждением сертифицированного оборудования (рентген-аппарат и УЗИ-аппарат) и штата дипломированных медицинских специалистов. Подтвержденный опыт медицинских услуг, включая обязательные медицинские осмотры, не менее 3 лет (копии актов/договоров). Обязательства по плановой вакцинации студентов и сотрудников, круглосуточной диспетчерской службе и мобильной бригаде, координации госпитализации и регулярной информационно-разъяснительной работе по ЗОЖ.');

-- Совпадающие объекты переиспользуются без изменений; название старого
-- объекта может отличаться (например, иметь уточнение «машфак»).
INSERT INTO core.objects(kind,name,name_kk,address,address_kk,area_m2)
SELECT 'premises',i.name_ru,i.name_kk,i.address_ru,i.address_kk,i.area_m2
FROM announcement_lots i
WHERE NOT EXISTS (
  SELECT 1 FROM core.objects o
  WHERE o.kind='premises' AND o.address=i.address_ru AND o.area_m2=i.area_m2
);

-- announced_at остается NULL до реальной публикации. Дату аукциона
-- документ не содержит, поэтому trading_at не заполняется.
-- repeat_of относится ко всему тендеру; здесь смешаны повторные и первичный
-- лоты, их обозначение из источника сохраняется в rate_calculation.
INSERT INTO core.tenders(
  id,title,title_kk,organizer_id,submission_deadline,opening_at
) VALUES (
  :'tender_id'::uuid,
  'Аренда недвижимости ТОУ — лоты №1, №8 и №11 (объявление от 12.10.2026)',
  'ТОУ жылжымайтын мүлкін жалға беру — №1, №8 және №11 лоттар (12.10.2026 хабарландыру)',
  :'actor_id'::uuid,
  '2026-10-22 18:00:00+05'::timestamptz,
  '2026-10-23 10:00:00+05'::timestamptz
);

INSERT INTO core.lots(
  tender_id,seq,object_id,purpose,purpose_kk,lease_months,
  base_rate_monthly,guarantee_fee,rate_calculation,rate_unit
)
SELECT :'tender_id'::uuid,i.seq,o.id,i.purpose_ru,i.purpose_kk,12,
  i.price,i.price,
  jsonb_build_object(
    'source','Объявление.docx',
    'source_announcement_date','2026-10-12',
    'source_lot_kind',i.source_kind,
    'method','approved_starting_price',
    'amount',i.price,'currency','KZT','rate_unit','monthly',
    'vat_included',false,
    'qualification_requirements',i.qualification_requirements,
    'source_guarantee_payment_deadline','2026-10-20'
  ),'monthly'
FROM announcement_lots i
JOIN LATERAL (
  SELECT existing.id FROM core.objects existing
  WHERE existing.kind='premises'
    AND existing.address=i.address_ru AND existing.area_m2=i.area_m2
  ORDER BY existing.created_at,existing.id LIMIT 1
) o ON true;

\endif
COMMIT;

SELECT t.id,t.status,t.title,
  t.submission_deadline AT TIME ZONE 'Asia/Almaty' AS deadline_local,
  t.opening_at AT TIME ZONE 'Asia/Almaty' AS opening_local,
  t.trading_at,count(l.id) AS lots
FROM core.tenders t LEFT JOIN core.lots l ON l.tender_id=t.id
WHERE t.id=:'tender_id'::uuid
GROUP BY t.id;

SELECT l.seq,o.name,o.area_m2,l.base_rate_monthly,l.guarantee_fee,l.lease_months
FROM core.lots l JOIN core.objects o ON o.id=l.object_id
WHERE l.tender_id=:'tender_id'::uuid ORDER BY l.seq;
