CREATE TABLE notification_settings (singleton INTEGER PRIMARY KEY CHECK(singleton=1), profile_json TEXT NOT NULL DEFAULT '{}');
INSERT INTO notification_settings(singleton) VALUES(1);
INSERT INTO schema_migrations(version,applied_at) VALUES(13,unixepoch());
