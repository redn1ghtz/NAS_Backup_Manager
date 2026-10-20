//! Local SQLite state. No source/destination data is touched by this module.
use rusqlite::Connection;
use std::{path::Path, time::Duration};

pub struct Database {
    pub connection: Connection,
}

#[derive(Debug)]
pub enum DatabaseError {
    RelativePath,
    Sqlite(rusqlite::Error),
    UnsupportedVersion(i64),
}

impl From<rusqlite::Error> for DatabaseError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}

impl Database {
    /// Maintenance must never create an empty state file if the original vanished.
    pub fn open_existing(path: &Path) -> Result<Self, DatabaseError> {
        if !path.is_absolute() {
            return Err(DatabaseError::RelativePath);
        }
        Self::initialize(Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )?)
    }
    pub fn open(path: &Path) -> Result<Self, DatabaseError> {
        if !path.is_absolute() {
            return Err(DatabaseError::RelativePath);
        }
        Self::initialize(Connection::open(path)?)
    }

    fn initialize(mut connection: Connection) -> Result<Self, DatabaseError> {
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        let exists:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_migrations')",[],|r|r.get(0))?;
        if exists {
            let version: i64 = connection.query_row(
                "SELECT coalesce(max(version),0) FROM schema_migrations",
                [],
                |r| r.get(0),
            )?;
            if version == 10 {
                return Ok(Self { connection });
            }
            if !(1..=9).contains(&version) {
                return Err(DatabaseError::UnsupportedVersion(version));
            }
        }
        // Serialize schema discovery and creation across GUI/headless processes.
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_migrations')",
            [], |row| row.get(0),
        )?;
        if !exists {
            transaction.execute_batch(crate::INITIAL_SCHEMA)?;
        }
        let version: i64 = transaction.query_row(
            "SELECT coalesce(max(version),0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )?;
        if !(1..=10).contains(&version) {
            return Err(DatabaseError::UnsupportedVersion(version));
        }
        if version == 1 {
            transaction.execute_batch(include_str!("../migrations/0002_backup.sql"))?;
        }
        if version <= 2 {
            transaction.execute_batch(include_str!("../migrations/0003_job_policies.sql"))?;
        }
        if version <= 3 {
            transaction.execute_batch(include_str!("../migrations/0004_direct_destination.sql"))?;
        }
        if version <= 4 {
            transaction.execute_batch(include_str!("../migrations/0005_deletion_guard.sql"))?;
        }
        if version <= 5 {
            transaction.execute_batch(include_str!("../migrations/0006_wake_computer.sql"))?;
        }
        if version <= 6 {
            transaction.execute_batch(include_str!("../migrations/0007_schedule_mode.sql"))?;
        }
        if version <= 7 {
            transaction.execute_batch(include_str!("../migrations/0008_activity.sql"))?;
        }
        if version <= 8 {
            transaction.execute_batch(include_str!("../migrations/0009_startup_schedule.sql"))?;
        }
        if version <= 9 {
            transaction.execute_batch(include_str!("../migrations/0010_verified_progress.sql"))?;
        }
        transaction.commit()?;
        Ok(Self { connection })
    }

    pub fn schema_version(&self) -> Result<i64, DatabaseError> {
        Ok(self
            .connection
            .query_row("SELECT max(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })?)
    }

    pub fn active_job_count(&self) -> Result<i64, DatabaseError> {
        Ok(self.connection.query_row(
            "SELECT count(*) FROM jobs WHERE removed_at IS NULL",
            [],
            |row| row.get(0),
        )?)
    }

    pub fn language(&self) -> Result<String, DatabaseError> {
        Ok(self.connection.query_row(
            "SELECT language FROM settings WHERE singleton=1",
            [],
            |row| row.get(0),
        )?)
    }

    pub fn notifications_enabled(&self) -> Result<bool, DatabaseError> {
        Ok(self.connection.query_row(
            "SELECT notifications_enabled FROM settings WHERE singleton=1",
            [],
            |r| r.get(0),
        )?)
    }
    pub fn set_notifications_enabled(&self, enabled: bool) -> Result<(), DatabaseError> {
        self.connection.execute(
            "UPDATE settings SET notifications_enabled=?1 WHERE singleton=1",
            [enabled],
        )?;
        Ok(())
    }

    pub fn set_language(&self, language: &str) -> Result<(), DatabaseError> {
        self.connection.execute(
            "UPDATE settings SET language=?1 WHERE singleton=1",
            [language],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_empty_database_and_reopens_without_losing_settings() {
        let db = Database::initialize(Connection::open_in_memory().unwrap()).unwrap();
        assert_eq!(db.schema_version().unwrap(), 10);
        assert_eq!(db.active_job_count().unwrap(), 0);
        assert_eq!(db.language().unwrap(), "ru");
        assert!(db.notifications_enabled().unwrap());
        db.set_notifications_enabled(false).unwrap();
        db.set_language("en").unwrap();
        let reopened = Database::initialize(db.connection).unwrap();
        assert_eq!(reopened.language().unwrap(), "en");
        assert!(!reopened.notifications_enabled().unwrap());
        assert!(reopened.set_language("invalid").is_err());
        assert_eq!(reopened.language().unwrap(), "en");
    }

    #[test]
    fn upgrades_v2_preserving_existing_jobs_and_defaults() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(crate::INITIAL_SCHEMA).unwrap();
        c.execute_batch(include_str!("../migrations/0002_backup.sql"))
            .unwrap();
        c.execute("INSERT INTO jobs(id,name,source_path,destination_path,destination_kind,retention_days,daily_minute,created_at,updated_at) VALUES('existing','keep','source','dest','local',45,725,1,1)",[]).unwrap();
        let db = Database::initialize(c).unwrap();
        assert_eq!(db.schema_version().unwrap(), 10);
        let values:(u32,u16,String,String,u32)=db.connection.query_row("SELECT retention_days,daily_minute,archive_mode,schedule_kind,schedule_every FROM jobs WHERE id='existing'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap();
        assert_eq!(values, (45, 725, "days".into(), "daily".into(), 1));
        assert_eq!(
            db.connection
                .query_row(
                    "SELECT destination_layout FROM jobs WHERE id='existing'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "isolated"
        );
        let guard: (bool, u32) = db.connection.query_row("SELECT deletion_guard_enabled,deletion_guard_min_files FROM jobs WHERE id='existing'", [], |r| Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(guard, (true, 10));
        assert!(!db
            .connection
            .query_row(
                "SELECT wake_computer FROM jobs WHERE id='existing'",
                [],
                |r| r.get::<_, bool>(0)
            )
            .unwrap());
        assert!(db
            .connection
            .execute("UPDATE jobs SET deletion_guard_min_files=0", [])
            .is_err());
        assert!(db
            .connection
            .execute("UPDATE jobs SET archive_mode='invalid'", [])
            .is_err());
    }

    #[test]
    fn migration_preserves_enabled_schedules_without_activating_disabled_jobs() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(crate::INITIAL_SCHEMA).unwrap();
        for sql in [
            include_str!("../migrations/0002_backup.sql"),
            include_str!("../migrations/0003_job_policies.sql"),
            include_str!("../migrations/0004_direct_destination.sql"),
            include_str!("../migrations/0005_deletion_guard.sql"),
            include_str!("../migrations/0006_wake_computer.sql"),
        ] {
            c.execute_batch(sql).unwrap();
        }
        c.execute("INSERT INTO jobs(id,name,source_path,destination_path,destination_kind,expected_identity,ownership_nonce,approved_revision,enabled,schedule_enabled,schedule_kind,schedule_every,created_at,updated_at) VALUES('active','active','source','dest','local','volume','owner',1,1,1,'minutes',10,1,1)",[]).unwrap();
        c.execute("INSERT INTO jobs(id,name,source_path,destination_path,destination_kind,schedule_kind,schedule_every,created_at,updated_at) VALUES('disabled','disabled','source2','dest2','local','hours',3,1,1)",[]).unwrap();
        let db = Database::initialize(c).unwrap();
        for (id, automatic, enabled, kind, every) in [
            ("active", true, true, "minutes", 10),
            ("disabled", false, false, "hours", 3),
        ] {
            let actual:(bool,bool,bool,String,u32)=db.connection.query_row("SELECT schedule_automatic,schedule_enabled,schedule_pending,schedule_kind,schedule_every FROM jobs WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap();
            assert_eq!(actual, (automatic, enabled, false, kind.into(), every));
        }
    }

    #[test]
    fn refuses_future_schema_without_resetting_it() {
        let db = Database::initialize(Connection::open_in_memory().unwrap()).unwrap();
        db.connection
            .execute("INSERT INTO schema_migrations VALUES(11,1)", [])
            .unwrap();
        assert!(matches!(
            Database::initialize(db.connection),
            Err(DatabaseError::UnsupportedVersion(11))
        ));
    }

    #[test]
    fn enables_foreign_keys_on_connection() {
        let db = Database::initialize(Connection::open_in_memory().unwrap()).unwrap();
        let enabled: i64 = db
            .connection
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(enabled, 1);
    }

    #[test]
    fn rejects_ambient_relative_database_path() {
        assert!(matches!(
            Database::open(Path::new("settings.sqlite")),
            Err(DatabaseError::RelativePath)
        ));
    }
}
