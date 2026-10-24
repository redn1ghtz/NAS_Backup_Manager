ALTER TABLE jobs ADD COLUMN advanced_json TEXT NOT NULL DEFAULT '{}';
CREATE TABLE file_integrity (
 job_id TEXT NOT NULL REFERENCES jobs(id), path TEXT NOT NULL,
 source_bytes INTEGER NOT NULL, source_modified TEXT NOT NULL,
 stored_bytes INTEGER NOT NULL, stored_modified TEXT NOT NULL,
 sha256 TEXT NOT NULL, verified_at INTEGER NOT NULL, codec TEXT NOT NULL DEFAULT '',
 PRIMARY KEY(job_id,path)
);
CREATE INDEX integrity_due ON file_integrity(job_id,verified_at,path);
CREATE TABLE integrity_runs (
 job_id TEXT PRIMARY KEY REFERENCES jobs(id), checked_at INTEGER NOT NULL,
 checked_files INTEGER NOT NULL, state TEXT NOT NULL, error_code TEXT
);
CREATE TABLE cloud_capacity (
 job_id TEXT PRIMARY KEY REFERENCES jobs(id), queried_at INTEGER NOT NULL,
 total INTEGER, used INTEGER, free INTEGER
);
CREATE TABLE cloud_integrity (
 job_id TEXT NOT NULL REFERENCES jobs(id), remote TEXT NOT NULL,
 path TEXT NOT NULL, sha256 TEXT NOT NULL, PRIMARY KEY(job_id,remote,path)
);
INSERT INTO schema_migrations(version,applied_at) VALUES(11,unixepoch());
