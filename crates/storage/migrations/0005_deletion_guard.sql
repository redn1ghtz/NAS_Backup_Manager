-- Count-based, independently configurable deletion protection per job.
ALTER TABLE jobs ADD COLUMN deletion_guard_enabled INTEGER NOT NULL DEFAULT 1 CHECK(deletion_guard_enabled IN (0,1));
ALTER TABLE jobs ADD COLUMN deletion_guard_min_files INTEGER NOT NULL DEFAULT 10 CHECK(deletion_guard_min_files BETWEEN 1 AND 4294967295);
INSERT INTO schema_migrations(version,applied_at) VALUES(5,unixepoch());
