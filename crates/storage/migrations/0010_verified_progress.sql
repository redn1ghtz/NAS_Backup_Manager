ALTER TABLE job_activity ADD COLUMN verified_bytes INTEGER NOT NULL DEFAULT 0 CHECK(verified_bytes>=0);
ALTER TABLE job_activity ADD COLUMN eta_seconds INTEGER CHECK(eta_seconds IS NULL OR eta_seconds>=0);
INSERT INTO schema_migrations(version,applied_at) VALUES(10,unixepoch());
