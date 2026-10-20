CREATE TABLE job_runtime (
    job_id TEXT PRIMARY KEY REFERENCES jobs(id) ON DELETE RESTRICT,
    source_identity TEXT NOT NULL,
    current_identity TEXT NOT NULL,
    archive_identity TEXT NOT NULL
);
CREATE TABLE scan_entries (
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    side INTEGER NOT NULL CHECK(side IN (0,1)),
    path TEXT NOT NULL,
    is_dir INTEGER NOT NULL,
    bytes INTEGER NOT NULL,
    modified TEXT NOT NULL,
    identity TEXT NOT NULL,
    PRIMARY KEY(run_id, side, path)
);
CREATE INDEX scan_paths ON scan_entries(run_id, path, side);
CREATE TABLE scan_directories (
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    side INTEGER NOT NULL,
    path TEXT NOT NULL,
    done INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(run_id,side,path)
);
CREATE TABLE previews (
    run_id TEXT PRIMARY KEY REFERENCES runs(id) ON DELETE RESTRICT,
    fingerprint TEXT NOT NULL,
    approved INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE audit_events (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE RESTRICT,
    kind TEXT NOT NULL,
    happened_at INTEGER NOT NULL,
    state TEXT NOT NULL DEFAULT 'succeeded',
    error_code TEXT
);
CREATE INDEX audit_page ON audit_events(happened_at DESC,id);
CREATE INDEX archive_run ON archive_versions(run_id);
CREATE INDEX jobs_active ON jobs(removed_at,created_at,id);
INSERT INTO schema_migrations(version, applied_at) VALUES(2, unixepoch());

CREATE INDEX operations_pending ON operations(run_id,kind,state,sequence);
