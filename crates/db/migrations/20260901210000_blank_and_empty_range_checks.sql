-- Пустая строка и пустой диапазон - не значения (NFR-01, INV-DB-02).
--
-- Круг 2 гаунтлета, пробы в одноразовой БД:
--
--   INSERT INTO core.objects (..., name, address, name_kk, address_kk)
--     VALUES (..., '', '', 'x', 'y');        -> INSERT 0 1
--   INSERT INTO core.tenders (..., title) VALUES (..., '');  -> принят
--   INSERT INTO core.users (..., full_name) VALUES (..., ''); -> принят
--
-- Симметрия нарушилась при добавлении казахских колонок: у `name_kk` и
-- `address_kk` проверки `btrim(...) <> ''` есть (20260828120000), а у
-- исходных русских - нет. Пустое наименование объекта уезжает в
-- объявление и в договор.
--
-- Отдельно - пустой `tstzrange`:
--
--   INSERT INTO core.contracts (..., lease_period = 'empty'::tstzrange) x2
--   -> оба приняты, у объекта два действующих договора
--
-- Пустой диапазон не пересекается ни с чем, поэтому EXCLUDE-ограничение
-- `no_overlapping_lease` (INV-DB-02) его пропускает: запрет «нет
-- пересекающихся аренд объекта» обходится значением, которое ничего не
-- обозначает.

ALTER TABLE core.objects
  ADD CONSTRAINT objects_name_not_blank    CHECK (btrim(name) <> ''),
  ADD CONSTRAINT objects_address_not_blank CHECK (btrim(address) <> '');

ALTER TABLE core.tenders
  ADD CONSTRAINT tenders_title_not_blank CHECK (btrim(title) <> '');

ALTER TABLE core.users
  ADD CONSTRAINT users_full_name_not_blank CHECK (btrim(full_name) <> '');

ALTER TABLE core.lots
  ADD CONSTRAINT lots_purpose_not_blank CHECK (btrim(purpose) <> '');

ALTER TABLE core.contracts
  ADD CONSTRAINT contracts_lease_period_not_empty
    CHECK (lease_period IS NULL OR NOT isempty(lease_period));
