-- Opt-in; existing jobs never gain a wake timer silently.
ALTER TABLE jobs ADD COLUMN wake_computer INTEGER NOT NULL DEFAULT 0 CHECK(wake_computer IN (0,1));
INSERT INTO schema_migrations(version,applied_at) VALUES(6,unixepoch());
