// Scheduled Windows runs must not create a console window.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    // Inno has already disabled this installation's OS triggers before invoking
    // this metadata-only step. Never create a missing database or delete backups.
    if args.len() == 4 && args[1] == "--state" && args[3] == "uninstall-mark-schedules-disabled" {
        return match mark_schedules_disabled(std::path::Path::new(&args[2])) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        };
    }
    if args.get(1).map(String::as_str) == Some("--state") {
        let outcome = (|| -> Result<(), String> {
            if args.len() != 5 || args[3] != "run-scheduled" {
                return Err("Usage: nas-backup --state ABSOLUTE_DB run-scheduled JOB_UUID".into());
            }
            let path = std::path::Path::new(&args[2]);
            if !path.is_absolute() || !path.is_file() {
                return Err("State database missing".into());
            }
            let db = nas_backup_storage::database::Database::open(path)
                .map_err(|_| "Database unavailable")?;
            let job =
                nas_backup_core::backup::get_job(&db.connection, &args[4]).map_err(|e| e.code)?;
            let mut feedback = nas_backup_core::activity::Feedback::new(&db.connection, &job, path);
            let result = nas_backup_core::backup::run_scheduled_progress(
                &db.connection,
                &args[4],
                |progress| feedback.progress(&progress),
            );
            feedback.finish(result.as_ref().err());
            result.map_err(|e| e.code)
        })();
        return match outcome {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) if error == "cancelled" => ExitCode::from(130),
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        };
    }
    match std::env::args().nth(1).as_deref() {
        Some("--version") => {
            println!("nas-backup {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("--help") | None => {
            println!("NAS Backup Manager\nUsage: nas-backup --version | --help\nScheduled execution: nas-backup --state ABSOLUTE_DB run-scheduled JOB_UUID");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("Unsupported command. Use --help.");
            ExitCode::FAILURE
        }
    }
}

fn mark_schedules_disabled(path: &std::path::Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("absolute_path_required".into());
    }
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Ok(metadata) if metadata.file_type().is_file() => {}
        _ => return Err("database_unavailable".into()),
    }
    let db = nas_backup_storage::database::Database::open_existing(path)
        .map_err(|_| "database_unavailable")?;
    if !nas_backup_core::activity::list(&db.connection)
        .map_err(|e| e.code)?
        .is_empty()
    {
        return Err("job_busy".into());
    }
    db.connection
        .execute(
            "UPDATE jobs SET schedule_enabled=0 WHERE schedule_enabled=1",
            [],
        )
        .map_err(|_| "database_unavailable")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nas_backup_core::backup::*;
    #[test]
    fn uninstall_metadata_is_idempotent_preserves_data_and_refuses_active_worker() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("state.sqlite");
        mark_schedules_disabled(&state).unwrap();
        assert!(!state.exists());
        assert!(mark_schedules_disabled(std::path::Path::new("relative.sqlite")).is_err());
        let db = nas_backup_storage::database::Database::open(&state).unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&destination).unwrap();
        let job = save_job(
            &db.connection,
            JobDraft {
                advanced: Default::default(),
                secrets: None,
                id: None,
                revision: None,
                name: "Uninstall QA".into(),
                source: source.to_string_lossy().into(),
                destination: destination.to_string_lossy().into(),
                kind: "local".into(),
                schedule_automatic: true,
                wake_computer: false,
                deletion_guard_enabled: true,
                deletion_guard_min_files: 10,
                retention_days: 30,
                archive_mode: "days".into(),
                schedule_kind: "daily".into(),
                schedule_every: 1,
                daily_minute: 180,
                missed_run: "skip".into(),
            },
        )
        .unwrap();
        std::fs::write(source.join("keep"), "source").unwrap();
        let plan = preview(&db.connection, &job.id).unwrap();
        execute(&db.connection, &job.id, &plan.id, true).unwrap();
        db.connection
            .execute("UPDATE jobs SET schedule_enabled=1 WHERE id=?1", [&job.id])
            .unwrap();
        let before = get_job(&db.connection, &job.id).unwrap();
        let active = nas_backup_core::activity::begin(&db.connection, &job.id).unwrap();
        assert_eq!(mark_schedules_disabled(&state).unwrap_err(), "job_busy");
        assert!(get_job(&db.connection, &job.id).unwrap().schedule_enabled);
        drop(active);
        mark_schedules_disabled(&state).unwrap();
        mark_schedules_disabled(&state).unwrap();
        let after = get_job(&db.connection, &job.id).unwrap();
        assert!(!after.schedule_enabled);
        assert_eq!(before.enabled, after.enabled);
        assert_eq!(before.revision, after.revision);
        assert_eq!(before.last_run, after.last_run);
        assert_eq!(std::fs::read(source.join("keep")).unwrap(), b"source");
        assert_eq!(std::fs::read(destination.join("keep")).unwrap(), b"source");
    }
}
