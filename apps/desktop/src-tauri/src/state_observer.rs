//! Observe local SQLite commits, including writes by the scheduled CLI.
use nas_backup_storage::database::Database;
use std::path::Path;

#[derive(Default)]
pub struct StateObserver {
    database: Option<Database>,
    version: Option<i64>,
}
impl StateObserver {
    pub fn changed(&mut self, path: &Path) -> Result<bool, String> {
        if self.database.is_none() {
            self.database = Some(Database::open(path).map_err(|_| "database_unavailable")?);
        }
        let version = self
            .database
            .as_ref()
            .ok_or("database_unavailable")?
            .connection
            .query_row("PRAGMA data_version", [], |row| row.get::<_, i64>(0))
            .map_err(|_| "database_unavailable")?;
        let changed = self.version != Some(version);
        self.version = Some(version);
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_external_commits_without_reporting_reads_or_rollbacks() {
        let path = std::env::temp_dir().join(format!(
            "nas-state-observer-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let owned = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        drop(owned);
        let mut observer = StateObserver::default();
        assert!(observer.changed(&path).unwrap());
        assert!(!observer.changed(&path).unwrap());
        let mut external = Database::open(&path).unwrap();
        assert!(!observer.changed(&path).unwrap());
        external.set_language("en").unwrap();
        assert!(observer.changed(&path).unwrap());
        assert!(!observer.changed(&path).unwrap());
        {
            let transaction = external.connection.transaction().unwrap();
            transaction
                .execute("UPDATE settings SET language='ru'", [])
                .unwrap();
            transaction.rollback().unwrap();
        }
        assert!(!observer.changed(&path).unwrap());
        drop(external);
        drop(observer);
        std::fs::remove_file(path).unwrap();
    }
}
