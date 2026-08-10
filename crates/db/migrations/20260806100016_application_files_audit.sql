-- Файлы заявок — тоже мутации домена (А.5): подключаем к hash-цепочке INV-A01.
-- Триггер отсутствовал в 13-й миграции, т.к. таблица наполнялась только с Т8.

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.application_files
  FOR EACH ROW EXECUTE FUNCTION audit.record();
