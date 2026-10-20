-- Existing data stays where it is. Only newly enrolled jobs use direct layout.
ALTER TABLE jobs ADD COLUMN destination_layout TEXT NOT NULL DEFAULT 'isolated'
    CHECK(destination_layout IN ('isolated','direct'));
INSERT INTO schema_migrations(version,applied_at) VALUES(4,unixepoch());
