-- Runner must set foreign_keys=ON on EVERY connection before BEGIN.
-- Run each migration and its version insert in one transaction.
CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at INTEGER NOT NULL
);

CREATE TABLE jobs (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK(length(trim(name)) > 0),
    source_path TEXT NOT NULL CHECK(length(source_path) > 0),
    destination_path TEXT NOT NULL CHECK(length(destination_path) > 0),
    destination_kind TEXT NOT NULL CHECK(destination_kind IN ('local', 'network')),
    expected_identity TEXT,
    ownership_nonce TEXT,
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision > 0),
    enabled INTEGER NOT NULL DEFAULT 0 CHECK(enabled IN (0, 1)),
    schedule_enabled INTEGER NOT NULL DEFAULT 0 CHECK(schedule_enabled IN (0, 1)),
    daily_minute INTEGER NOT NULL DEFAULT 180 CHECK(daily_minute BETWEEN 0 AND 1439),
    missed_run_policy TEXT NOT NULL DEFAULT 'skip'
        CHECK(missed_run_policy IN ('skip', 'run_once_when_available')),
    retention_days INTEGER NOT NULL DEFAULT 30 CHECK(retention_days BETWEEN 1 AND 4294967295),
    -- NULL until a preview for the current revision has been explicitly approved.
    approved_revision INTEGER,
    removed_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CHECK(approved_revision IS NULL OR approved_revision = revision),
    CHECK(enabled = 0 OR (approved_revision = revision AND approved_revision IS NOT NULL
        AND expected_identity IS NOT NULL AND ownership_nonce IS NOT NULL)),
    CHECK(schedule_enabled = 0 OR enabled = 1),
    CHECK(removed_at IS NULL OR (enabled = 0 AND schedule_enabled = 0))
);
CREATE INDEX jobs_schedule ON jobs(schedule_enabled, removed_at, id);

CREATE TABLE runs (
    id TEXT PRIMARY KEY NOT NULL,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE RESTRICT,
    job_revision INTEGER NOT NULL CHECK(job_revision > 0),
    trigger_kind TEXT NOT NULL CHECK(trigger_kind IN ('preview', 'manual', 'scheduled')),
    state TEXT NOT NULL CHECK(state IN ('scanning', 'planned', 'running', 'succeeded', 'failed', 'interrupted')),
    started_at INTEGER NOT NULL,
    finished_at INTEGER,
    scan_complete INTEGER NOT NULL DEFAULT 0 CHECK(scan_complete IN (0, 1)),
    error_code TEXT,
    UNIQUE(id, job_id),
    CHECK(finished_at IS NULL OR finished_at >= started_at),
    CHECK(state != 'succeeded' OR (scan_complete = 1 AND finished_at IS NOT NULL AND error_code IS NULL))
);
CREATE INDEX runs_page ON runs(job_id, started_at DESC, id);
-- Preview and execution cannot race within one job. OS locks are also required.
CREATE UNIQUE INDEX runs_one_active_job ON runs(job_id)
    WHERE state IN ('scanning', 'planned', 'running');

CREATE TABLE operations (
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE RESTRICT,
    sequence INTEGER NOT NULL CHECK(sequence >= 0),
    relative_path TEXT NOT NULL CHECK(length(relative_path) > 0),
    kind TEXT NOT NULL CHECK(kind IN ('create', 'update', 'unchanged', 'archive_remove', 'mkdir')),
    state TEXT NOT NULL CHECK(state IN ('planned', 'staged', 'archived', 'committed', 'failed')),
    byte_count INTEGER NOT NULL CHECK(byte_count >= 0),
    error_code TEXT,
    PRIMARY KEY(run_id, sequence)
);

CREATE TABLE archive_versions (
    id TEXT PRIMARY KEY NOT NULL,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE RESTRICT,
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE RESTRICT,
    original_relative_path TEXT NOT NULL CHECK(length(original_relative_path) > 0),
    -- Relative to the job's isolated archive root; validate before all filesystem use.
    stored_relative_path TEXT NOT NULL,
    reason TEXT NOT NULL CHECK(reason IN ('deleted', 'replaced')),
    archived_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL CHECK(expires_at > archived_at),
    byte_count INTEGER NOT NULL CHECK(byte_count >= 0),
    content_hash TEXT,
    state TEXT NOT NULL CHECK(state IN ('pending', 'verified', 'restoring', 'expiring', 'expired', 'failed')),
    UNIQUE(job_id, stored_relative_path),
    FOREIGN KEY(run_id, job_id) REFERENCES runs(id, job_id) ON DELETE RESTRICT
);
CREATE INDEX archive_page ON archive_versions(job_id, archived_at DESC, id);
CREATE INDEX archive_expiry ON archive_versions(state, expires_at, id);

CREATE TABLE settings (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    language TEXT NOT NULL DEFAULT 'ru' CHECK(language IN ('ru', 'en'))
);
INSERT INTO settings(singleton) VALUES (1);
INSERT INTO schema_migrations(version, applied_at) VALUES (1, unixepoch());
