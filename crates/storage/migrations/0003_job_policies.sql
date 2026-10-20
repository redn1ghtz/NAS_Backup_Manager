-- Keep positive retention_days for existing versions; opting out never purges archives.
ALTER TABLE jobs ADD COLUMN archive_mode TEXT NOT NULL DEFAULT 'days' CHECK(archive_mode IN ('days','none'));
ALTER TABLE jobs ADD COLUMN schedule_kind TEXT NOT NULL DEFAULT 'daily' CHECK(schedule_kind IN ('daily','minutes','hours','days'));
ALTER TABLE jobs ADD COLUMN schedule_every INTEGER NOT NULL DEFAULT 1 CHECK(schedule_every BETWEEN 1 AND 44640);
ALTER TABLE jobs ADD COLUMN schedule_anchor TEXT NOT NULL DEFAULT '1970-01-01';
CREATE TABLE run_metrics (
    run_id TEXT PRIMARY KEY REFERENCES runs(id) ON DELETE CASCADE,
    created_count INTEGER NOT NULL,
    updated_count INTEGER NOT NULL,
    deleted_count INTEGER NOT NULL,
    unchanged_count INTEGER NOT NULL,
    created_bytes INTEGER NOT NULL,
    updated_bytes INTEGER NOT NULL,
    deleted_bytes INTEGER NOT NULL
);
-- Bounded event journal survives pruning of temporary preview/scan metadata.
CREATE TABLE file_events (
    id INTEGER PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE RESTRICT,
    relative_path TEXT NOT NULL,
    kind TEXT NOT NULL,
    state TEXT NOT NULL,
    byte_count INTEGER NOT NULL,
    happened_at INTEGER NOT NULL
);
CREATE INDEX file_events_job_page ON file_events(job_id,id DESC);
INSERT INTO schema_migrations(version,applied_at) VALUES(3,unixepoch());
