-- Keep the codec with the immutable version, independently of later job edits.
ALTER TABLE archive_versions ADD COLUMN codec TEXT NOT NULL DEFAULT '';
INSERT INTO schema_migrations(version,applied_at) VALUES(12,unixepoch());
