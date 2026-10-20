-- Preserve existing schedules; do not automatically enroll previously paused jobs.
ALTER TABLE jobs ADD COLUMN schedule_automatic INTEGER NOT NULL DEFAULT 0 CHECK(schedule_automatic IN (0,1));
ALTER TABLE jobs ADD COLUMN schedule_pending INTEGER NOT NULL DEFAULT 0 CHECK(schedule_pending IN (0,1));
UPDATE jobs SET schedule_automatic=schedule_enabled;
CREATE TRIGGER manual_schedule_insert BEFORE INSERT ON jobs
WHEN NEW.schedule_enabled=1 AND NEW.schedule_automatic=0 BEGIN SELECT RAISE(ABORT,'manual schedule'); END;
CREATE TRIGGER manual_schedule_update BEFORE UPDATE ON jobs
WHEN NEW.schedule_enabled=1 AND NEW.schedule_automatic=0 BEGIN SELECT RAISE(ABORT,'manual schedule'); END;
INSERT INTO schema_migrations(version,applied_at) VALUES(7,unixepoch());
