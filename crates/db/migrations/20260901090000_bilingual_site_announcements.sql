-- Russian remains the canonical value used by existing clients; Kazakh is
-- stored explicitly for the bilingual public home page. Existing content is
-- copied so the migration does not unpublish the current announcement.
ALTER TABLE core.site_announcements
  ADD COLUMN title_kk text,
  ADD COLUMN body_kk text;

UPDATE core.site_announcements
SET title_kk = title,
    body_kk = body;

ALTER TABLE core.site_announcements
  ALTER COLUMN title_kk SET NOT NULL,
  ALTER COLUMN body_kk SET NOT NULL,
  ADD CONSTRAINT site_announcements_title_kk_length
    CHECK (char_length(btrim(title_kk)) BETWEEN 1 AND 200),
  ADD CONSTRAINT site_announcements_body_kk_length
    CHECK (char_length(btrim(body_kk)) BETWEEN 1 AND 20000);
