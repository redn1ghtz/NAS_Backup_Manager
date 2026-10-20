-- Keep legacy interval constraints intact; expose this mode as 'startup' over IPC.
ALTER TABLE jobs ADD COLUMN schedule_on_login INTEGER NOT NULL DEFAULT 0
    CHECK(schedule_on_login IN (0,1))
    CHECK(schedule_on_login=0 OR (schedule_kind='daily' AND schedule_every=1 AND wake_computer=0));
INSERT INTO schema_migrations(version,applied_at) VALUES(9,unixepoch());
