-- FR-1601/FR-1602: audit and document integrity must survive replica mode.
ALTER TABLE core.commission_documents ENABLE ALWAYS TRIGGER audit_record;
ALTER TABLE core.commission_documents ENABLE ALWAYS TRIGGER check_commission_document;
ALTER TABLE core.commission_documents ENABLE ALWAYS TRIGGER dossier_on_commission_document;
