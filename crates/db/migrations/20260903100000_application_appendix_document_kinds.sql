-- Приложения 9 и 11 загружаются отдельными обязательными PDF, а не
-- смешиваются с заявкой (Прил. 2) и подтверждающими квалификацию файлами.

ALTER TYPE core.application_document_kind
  ADD VALUE IF NOT EXISTS 'price_proposal_form';

ALTER TYPE core.application_document_kind
  ADD VALUE IF NOT EXISTS 'qualification_form';
