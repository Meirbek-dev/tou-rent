-- Реестр объектов имущества (М1). Статус объекта НЕ хранится (FR-103):
-- вычисляется из договоров и тендеров (view core.object_statuses).

CREATE TABLE core.objects (
  id                 uuid             PRIMARY KEY DEFAULT uuidv7(),
  kind               core.object_kind NOT NULL,
  name               text             NOT NULL,
  address            text             NOT NULL,
  area_m2            numeric(10,2)    NOT NULL CHECK (area_m2 > 0),
  floor_part         text,      -- этаж / часть здания
  -- Характеристики для коэффициентов Прил. 4 (FR-101): коды опций refdata.rate_coefficients
  premises_type_code text,      -- тип помещения (Кт)
  premises_kind_code text,      -- вид помещения
  comfort_code       text,      -- комфортность (Кк)
  location_code      text,      -- расположение (Кр)
  photo_keys         text[]     NOT NULL DEFAULT '{}',  -- ключи RustFS
  created_at         timestamptz NOT NULL DEFAULT now(),
  updated_at         timestamptz NOT NULL DEFAULT now()
);
COMMENT ON TABLE core.objects IS
  'Реестр имущества (FR-101, п. 3, 5). Удаление объекта с договором блокирует FK contracts.object_id';

CREATE TRIGGER touch_updated_at BEFORE UPDATE ON core.objects
  FOR EACH ROW EXECUTE FUNCTION core.touch_updated_at();
