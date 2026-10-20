-- Local, bounded live execution state; never stored on the destination/NAS.
CREATE TABLE job_activity (
 job_id TEXT PRIMARY KEY REFERENCES jobs(id) ON DELETE CASCADE,
 token TEXT NOT NULL, run_id TEXT, phase TEXT NOT NULL,
 completed INTEGER NOT NULL DEFAULT 0, total INTEGER NOT NULL DEFAULT 0,
 copied_bytes INTEGER NOT NULL DEFAULT 0, total_bytes INTEGER NOT NULL DEFAULT 0,
 current_path TEXT, updated_at INTEGER NOT NULL
);
ALTER TABLE settings ADD COLUMN notifications_enabled INTEGER NOT NULL DEFAULT 1 CHECK(notifications_enabled IN (0,1));
INSERT INTO schema_migrations(version,applied_at) VALUES(8,unixepoch());
