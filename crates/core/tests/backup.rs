use nas_backup_core::backup::*;
use rusqlite::Connection;
use std::fs;

fn fixture() -> (tempfile::TempDir, Connection, SavedJob) {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&destination).unwrap();
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    db.execute_batch(include_str!("../../storage/migrations/0001_initial.sql"))
        .unwrap();
    db.execute_batch(include_str!("../../storage/migrations/0002_backup.sql"))
        .unwrap();
    db.execute_batch(include_str!(
        "../../storage/migrations/0003_job_policies.sql"
    ))
    .unwrap();
    db.execute_batch(include_str!(
        "../../storage/migrations/0004_direct_destination.sql"
    ))
    .unwrap();
    db.execute_batch(include_str!(
        "../../storage/migrations/0005_deletion_guard.sql"
    ))
    .unwrap();
    db.execute_batch(include_str!(
        "../../storage/migrations/0006_wake_computer.sql"
    ))
    .unwrap();
    db.execute_batch(include_str!(
        "../../storage/migrations/0007_schedule_mode.sql"
    ))
    .unwrap();
    db.execute_batch(include_str!("../../storage/migrations/0008_activity.sql"))
        .unwrap();
    db.execute_batch(include_str!(
        "../../storage/migrations/0009_startup_schedule.sql"
    ))
    .unwrap();
    db.execute_batch(include_str!(
        "../../storage/migrations/0010_verified_progress.sql"
    ))
    .unwrap();
    db.execute_batch(include_str!(
        "../../storage/migrations/0011_advanced_jobs.sql"
    ))
    .unwrap();
    db.execute_batch(include_str!(
        "../../storage/migrations/0012_archive_codec.sql"
    ))
    .unwrap();
    let draft = JobDraft {
        advanced: Default::default(),
        secrets: None,
        id: None,
        revision: None,
        name: "Документы".into(),
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
    };
    let job = save_job(&db, draft).unwrap();
    (temp, db, job)
}

fn service_root(job: &SavedJob) -> std::path::PathBuf {
    let root = std::path::Path::new(&job.destination).join(".nas-backup-manager");
    if job.destination_layout == "direct" {
        root
    } else {
        root.join(&job.id)
    }
}

fn approved_run(db: &Connection, job: &SavedJob) {
    let p = preview(db, &job.id).unwrap();
    execute(db, &job.id, &p.id, true).unwrap();
}

fn cancellation_fixture() -> (
    tempfile::TempDir,
    nas_backup_storage::database::Database,
    SavedJob,
) {
    let (temp, memory, job) = fixture();
    let path = temp.path().join("state.sqlite");
    memory
        .execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    let database = nas_backup_storage::database::Database::open(&path).unwrap();
    (temp, database, job)
}

#[test]
fn cancellation_discards_only_own_staging_and_preserves_previous_backup() {
    use nas_backup_core::activity;
    for mode in ["days", "none"] {
        let (temp, database, job) = cancellation_fixture();
        let db = &database.connection;
        db.execute(
            "UPDATE jobs SET archive_mode=?2 WHERE id=?1",
            (&job.id, mode),
        )
        .unwrap();
        let source = std::path::Path::new(&job.source);
        fs::write(source.join("a-large.bin"), b"previous valid backup").unwrap();
        fs::write(source.join("z-delete.txt"), b"keep until an approved run").unwrap();
        approved_run(db, &job);
        fs::File::create(source.join("a-large.bin"))
            .unwrap()
            .set_len(16 * 1024 * 1024)
            .unwrap();
        fs::write(source.join("b-new.txt"), b"not copied after cancellation").unwrap();
        fs::remove_file(source.join("z-delete.txt")).unwrap();
        let plan = preview(db, &job.id).unwrap();
        let controller =
            nas_backup_storage::database::Database::open(&temp.path().join("state.sqlite"))
                .unwrap();
        let mut requested = false;
        let mut old_token = String::new();
        let result = execute_progress(db, &job.id, &plan.id, true, |p| {
            if p.phase == "verifying" && !requested {
                old_token = activity::list(&controller.connection).unwrap()[0]
                    .execution_id
                    .clone();
                assert!(
                    activity::request_cancel(&controller.connection, &job.id, &old_token).unwrap()
                );
                assert!(
                    activity::request_cancel(&controller.connection, &job.id, &old_token).unwrap()
                );
                assert!(activity::list(&controller.connection).unwrap()[0].cancel_requested);
                requested = true;
            }
        });
        assert!(requested);
        assert_eq!(result.unwrap_err().code, "cancelled");
        let destination = std::path::Path::new(&job.current_path);
        assert_eq!(
            fs::read(destination.join("a-large.bin")).unwrap(),
            b"previous valid backup"
        );
        assert_eq!(
            fs::read(destination.join("z-delete.txt")).unwrap(),
            b"keep until an approved run"
        );
        assert!(!destination.join("b-new.txt").exists());
        assert_eq!(
            fs::read_dir(service_root(&job).join("staging"))
                .unwrap()
                .count(),
            0
        );
        assert!(list_archive(db, &job.id, 0).unwrap().is_empty());
        assert!(activity::list(db).unwrap().is_empty());
        assert!(!activity::request_cancel(db, &job.id, &old_token).unwrap());
        assert!(history_filtered(db, 0, true).unwrap().is_empty());
        assert_eq!(
            get_job(db, &job.id).unwrap().last_error.as_deref(),
            Some("cancelled")
        );
        let plan = preview(db, &job.id).unwrap();
        execute_progress(db, &job.id, &plan.id, true, |_| {
            assert!(
                !activity::request_cancel(&controller.connection, &job.id, &old_token).unwrap()
            );
        })
        .unwrap();
        assert!(destination.join("b-new.txt").is_file());
        assert!(!destination.join("z-delete.txt").exists());
    }
}

#[test]
fn cancellation_keeps_committed_files_and_scheduled_scan_is_neutral() {
    use nas_backup_core::activity;
    let (temp, database, job) = cancellation_fixture();
    let db = &database.connection;
    let source = std::path::Path::new(&job.source);
    fs::write(source.join("a-committed.txt"), "already copied").unwrap();
    fs::File::create(source.join("b-large.bin"))
        .unwrap()
        .set_len(16 * 1024 * 1024)
        .unwrap();
    let plan = preview(db, &job.id).unwrap();
    let controller =
        nas_backup_storage::database::Database::open(&temp.path().join("state.sqlite")).unwrap();
    let result = execute_progress(db, &job.id, &plan.id, true, |p| {
        if p.phase == "verifying" {
            let active = activity::list(&controller.connection).unwrap();
            activity::request_cancel(&controller.connection, &job.id, &active[0].execution_id)
                .unwrap();
        }
    });
    assert_eq!(result.unwrap_err().code, "cancelled");
    assert_eq!(
        fs::read(std::path::Path::new(&job.current_path).join("a-committed.txt")).unwrap(),
        b"already copied"
    );
    assert!(!std::path::Path::new(&job.current_path)
        .join("b-large.bin")
        .exists());
    approved_run(db, &job);
    db.execute("UPDATE jobs SET schedule_enabled=1 WHERE id=?1", [&job.id])
        .unwrap();
    fs::write(source.join("c-scheduled.txt"), "must not transfer").unwrap();
    let result = run_scheduled_progress(db, &job.id, |_| {
        let active = activity::list(&controller.connection).unwrap();
        activity::request_cancel(&controller.connection, &job.id, &active[0].execution_id).unwrap();
    });
    assert_eq!(result.unwrap_err().code, "cancelled");
    let job = get_job(db, &job.id).unwrap();
    assert!(job.enabled && job.schedule_enabled);
    assert_eq!(job.last_error.as_deref(), Some("cancelled"));
    assert!(!std::path::Path::new(&job.current_path)
        .join("c-scheduled.txt")
        .exists());
    assert!(activity::list(db).unwrap().is_empty());
    assert!(history_filtered(db, 0, true).unwrap().is_empty());
}

#[test]
fn new_changed_unchanged_deleted_and_restore() {
    let (_temp, db, job) = fixture();
    let source = std::path::Path::new(&job.source);
    fs::create_dir(source.join("Фото")).unwrap();
    fs::write(source.join("Фото/a.txt"), "first").unwrap();
    let p = preview(&db, &job.id).unwrap();
    assert_eq!(p.created, 1);
    assert!(!std::path::Path::new(&job.current_path)
        .join("Фото/a.txt")
        .exists());
    execute(&db, &job.id, &p.id, true).unwrap();
    let p = preview(&db, &job.id).unwrap();
    assert_eq!(p.unchanged, 1);
    assert_eq!(p.updated, 0);
    fs::write(source.join("Фото/a.txt"), "second version").unwrap();
    assert!(execute(&db, &job.id, &p.id, true).is_err()); // stale preview
    approved_run(&db, &job);
    fs::remove_file(source.join("Фото/a.txt")).unwrap();
    approved_run(&db, &job);
    assert!(!std::path::Path::new(&job.current_path)
        .join("Фото/a.txt")
        .exists());
    let entries = list_archive(&db, &job.id, 0).unwrap();
    assert_eq!(entries.len(), 2);
    restore(&db, &entries[0].id, false).unwrap();
    assert!(restore(&db, &entries[0].id, false).is_err());
    assert!(std::path::Path::new(&job.current_path)
        .join("Фото/a.txt")
        .exists());
}

#[test]
fn rejects_overlap_and_invalid_retention() {
    let (_temp, db, job) = fixture();
    let draft = JobDraft {
        advanced: Default::default(),
        secrets: None,
        id: None,
        revision: None,
        name: "bad".into(),
        source: job.source.clone(),
        destination: job.destination.clone(),
        kind: "local".into(),
        schedule_automatic: true,
        wake_computer: false,
        deletion_guard_enabled: true,
        deletion_guard_min_files: 10,
        retention_days: 30,
        archive_mode: "days".into(),
        schedule_kind: "daily".into(),
        schedule_every: 1,
        daily_minute: 0,
        missed_run: "skip".into(),
    };
    assert!(save_job(&db, draft.clone()).is_err());
    let mut invalid = draft;
    invalid.retention_days = 0;
    assert!(save_job(&db, invalid).is_err());
}

#[test]
#[cfg(windows)]
fn network_destination_cannot_enroll_an_unmounted_local_folder() {
    let (_temp, db, job) = fixture();
    let draft = JobDraft {
        advanced: Default::default(),
        secrets: None,
        id: None,
        revision: None,
        name: "unmounted NAS".into(),
        source: job.source,
        destination: job.destination,
        kind: "network".into(),
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
    };
    assert_eq!(
        save_job(&db, draft).unwrap_err().code,
        "destination_type_mismatch"
    );
}

#[test]
fn offline_source_never_becomes_empty_and_archive_failure_preserves_active() {
    let (temp, db, job) = fixture();
    fs::write(std::path::Path::new(&job.source).join("a"), "valid copy").unwrap();
    approved_run(&db, &job);
    fs::rename(&job.source, temp.path().join("offline")).unwrap();
    assert!(preview(&db, &job.id).is_err());
    assert_eq!(
        fs::read(std::path::Path::new(&job.current_path).join("a")).unwrap(),
        b"valid copy"
    );
    fs::rename(temp.path().join("offline"), &job.source).unwrap();
    fs::remove_file(std::path::Path::new(&job.source).join("a")).unwrap();
    let archive = service_root(&job).join("archive");
    fs::rename(&archive, temp.path().join("old-archive")).unwrap();
    fs::write(&archive, "blocked").unwrap();
    assert!(preview(&db, &job.id).is_err());
    assert!(std::path::Path::new(&job.current_path).join("a").exists());
}

#[test]
fn user_consent_required_and_unexpired_archive_is_preserved() {
    let (_temp, db, job) = fixture();
    fs::write(std::path::Path::new(&job.source).join("a"), "content").unwrap();
    let p = preview(&db, &job.id).unwrap();
    assert!(execute(&db, &job.id, &p.id, false).is_err());
    execute(&db, &job.id, &p.id, true).unwrap();
    fs::remove_file(std::path::Path::new(&job.source).join("a")).unwrap();
    approved_run(&db, &job);
    assert_eq!(cleanup(&db, &job.id, true).unwrap(), 0);
    assert_eq!(list_archive(&db, &job.id, 0).unwrap().len(), 1);
}

#[test]
fn expiry_does_not_touch_active_files_or_versions_from_failed_runs() {
    let (_temp, db, job) = fixture();
    let source = std::path::Path::new(&job.source).join("a");
    fs::write(&source, "old").unwrap();
    approved_run(&db, &job);
    fs::write(&source, "replacement").unwrap();
    approved_run(&db, &job);
    let version = list_archive(&db, &job.id, 0).unwrap().remove(0);
    db.execute(
        "UPDATE archive_versions SET archived_at=0,expires_at=1 WHERE id=?1",
        [&version.id],
    )
    .unwrap();
    assert_eq!(cleanup(&db, &job.id, true).unwrap(), 1);
    assert_eq!(
        fs::read(std::path::Path::new(&job.current_path).join("a")).unwrap(),
        b"replacement"
    );
    fs::write(&source, "third version").unwrap();
    approved_run(&db, &job);
    db.execute("UPDATE runs SET state='failed',error_code='injected' WHERE id IN (SELECT run_id FROM archive_versions WHERE state='verified')",[]).unwrap();
    approved_run(&db, &job);
    db.execute(
        "UPDATE archive_versions SET archived_at=0,expires_at=1 WHERE state='verified'",
        [],
    )
    .unwrap();
    assert_eq!(cleanup(&db, &job.id, true).unwrap(), 0);
    assert_eq!(list_archive(&db, &job.id, 0).unwrap().len(), 1);
}

#[test]
fn tampered_identity_and_owner_block_deletions() {
    let (temp, db, job) = fixture();
    let source = std::path::Path::new(&job.source).join("a");
    fs::write(&source, "keep me").unwrap();
    approved_run(&db, &job);
    fs::remove_file(&source).unwrap();
    let current = std::path::Path::new(&job.current_path);
    fs::rename(current, temp.path().join("original-current")).unwrap();
    fs::create_dir(current).unwrap();
    assert!(preview(&db, &job.id).is_err());
    fs::remove_dir(current).unwrap();
    fs::rename(temp.path().join("original-current"), current).unwrap();
    fs::write(service_root(&job).join("owner"), "wrong owner").unwrap();
    assert_eq!(
        preview(&db, &job.id).unwrap_err().code,
        "ownership_mismatch"
    );
    assert!(current.join("a").is_file());
}

#[test]
fn lock_prevents_duplicate_manual_or_scheduled_work() {
    let (_temp, db, job) = fixture();
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(service_root(&job).join("lock"))
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&file).unwrap();
    assert_eq!(preview(&db, &job.id).unwrap_err().code, "busy");
}

#[test]
fn directories_unicode_long_paths_and_repeated_deletions() {
    let (_temp, db, job) = fixture();
    let source = std::path::Path::new(&job.source);
    fs::create_dir(source.join("пустая")).unwrap();
    approved_run(&db, &job);
    assert!(std::path::Path::new(&job.current_path)
        .join("пустая")
        .is_dir());
    fs::remove_dir(source.join("пустая")).unwrap();
    approved_run(&db, &job);
    assert!(!std::path::Path::new(&job.current_path)
        .join("пустая")
        .exists());
    let relative = std::path::PathBuf::from("документы")
        .join("a".repeat(120))
        .join("b".repeat(120))
        .join("данные.txt");
    fs::create_dir_all(source.join(&relative).parent().unwrap()).unwrap();
    for value in ["first", "second"] {
        fs::write(source.join(&relative), value).unwrap();
        approved_run(&db, &job);
        fs::remove_file(source.join(&relative)).unwrap();
        approved_run(&db, &job);
    }
    let versions = list_archive(&db, &job.id, 0).unwrap();
    assert_eq!(versions.len(), 2);
    assert_ne!(versions[0].id, versions[1].id);
}

#[test]
fn removed_job_archive_is_recoverable_with_source_offline() {
    let (temp, db, job) = fixture();
    let source = std::path::Path::new(&job.source).join("a");
    fs::write(&source, "recover").unwrap();
    approved_run(&db, &job);
    fs::remove_file(&source).unwrap();
    approved_run(&db, &job);
    let version = list_archive(&db, &job.id, 0).unwrap().remove(0);
    remove_job(&db, &job.id).unwrap();
    assert!(list_jobs(&db, 0).unwrap().is_empty());
    assert_eq!(archive_jobs(&db, 0).unwrap().len(), 1);
    fs::rename(&job.source, temp.path().join("offline")).unwrap();
    restore(&db, &version.id, false).unwrap();
    assert_eq!(
        fs::read(std::path::Path::new(&job.current_path).join("a")).unwrap(),
        b"recover"
    );
}

#[test]
fn collision_confirmation_binds_to_the_observed_target_and_archives_it() {
    let (_temp, db, job) = fixture();
    let source = std::path::Path::new(&job.source).join("a");
    let target = std::path::Path::new(&job.current_path).join("a");
    fs::write(&source, "old").unwrap();
    approved_run(&db, &job);
    fs::write(&source, "second").unwrap();
    approved_run(&db, &job);
    let version = list_archive(&db, &job.id, 0).unwrap().remove(0);
    let challenge = restore_confirmed(&db, &version.id, None).unwrap_err();
    assert_eq!(challenge.code, "restore_collision");
    fs::write(&target, "external edit").unwrap();
    assert!(restore_confirmed(&db, &version.id, Some(&challenge.detail)).is_err());
    let challenge = restore_confirmed(&db, &version.id, None).unwrap_err();
    restore_confirmed(&db, &version.id, Some(&challenge.detail)).unwrap();
    assert_eq!(fs::read(target).unwrap(), b"old");
    assert_eq!(list_archive(&db, &job.id, 0).unwrap().len(), 2);
}

#[test]
fn scheduled_run_refuses_mass_deletion() {
    let (_temp, db, job) = fixture();
    db.execute(
        "UPDATE jobs SET deletion_guard_min_files=1 WHERE id=?1",
        [&job.id],
    )
    .unwrap();
    let source = std::path::Path::new(&job.source).join("a");
    fs::write(&source, "keep").unwrap();
    approved_run(&db, &job);
    db.execute("UPDATE jobs SET schedule_enabled=1 WHERE id=?1", [&job.id])
        .unwrap();
    fs::remove_file(&source).unwrap();
    assert_eq!(
        run_scheduled(&db, &job.id).unwrap_err().code,
        "mass_deletion"
    );
    assert!(std::path::Path::new(&job.current_path).join("a").is_file());
}

#[test]
fn large_file_is_streamed_and_progress_counts_committed_files() {
    let (_temp, db, job) = fixture();
    let source = std::path::Path::new(&job.source).join("large.bin");
    use std::io::{Read, Write};
    let chunk = [0x5au8; 65536];
    let mut file = fs::File::create(&source).unwrap();
    for _ in 0..128 {
        file.write_all(&chunk).unwrap();
    }
    drop(file);
    let p = preview(&db, &job.id).unwrap();
    let mut last = (0, 0);
    execute_progress(&db, &job.id, &p.id, true, |progress| {
        last = (progress.completed, progress.total)
    })
    .unwrap();
    assert_eq!(last, (1, 1));
    let mut output =
        fs::File::open(std::path::Path::new(&job.current_path).join("large.bin")).unwrap();
    let mut read = [0u8; 65536];
    let mut total = 0;
    loop {
        let n = output.read(&mut read).unwrap();
        if n == 0 {
            break;
        }
        assert_eq!(&read[..n], &chunk[..n]);
        total += n;
    }
    assert_eq!(total, 8 * 1024 * 1024);
}

#[test]
fn recovery_only_removes_owned_staging_and_marks_interruption() {
    let (_temp, db, job) = fixture();
    let source = std::path::Path::new(&job.source).join("a");
    fs::write(&source, "recover source").unwrap();
    let preview = preview(&db, &job.id).unwrap();
    let sequence: i64 = db
        .query_row(
            "SELECT sequence FROM operations WHERE run_id=?1 LIMIT 1",
            [&preview.id],
            |r| r.get(0),
        )
        .unwrap();
    let stage = service_root(&job)
        .join("staging")
        .join(format!("{}-{sequence}.tmp", preview.id));
    fs::write(&stage, "interrupted staged bytes").unwrap();
    let unrelated = stage.parent().unwrap().join("do-not-remove.txt");
    fs::write(&unrelated, "unrelated").unwrap();
    db.execute(
        "UPDATE runs SET state='running',finished_at=NULL WHERE id=?1",
        [&preview.id],
    )
    .unwrap();
    recover_incomplete(&db).unwrap();
    assert!(!stage.exists());
    assert!(unrelated.exists());
    let state: String = db
        .query_row("SELECT state FROM runs WHERE id=?1", [preview.id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(state, "interrupted");
    assert_eq!(fs::read(source).unwrap(), b"recover source");
}

#[test]
#[cfg(windows)]
fn exclusive_file_lock_never_corrupts_the_previous_backup() {
    let (_temp, db, job) = fixture();
    let source = std::path::Path::new(&job.source).join("database.db");
    fs::write(&source, "old valid copy").unwrap();
    approved_run(&db, &job);
    fs::write(&source, "new database being written").unwrap();
    use std::os::windows::fs::OpenOptionsExt;
    let _locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&source)
        .unwrap();
    match preview(&db, &job.id) {
        Err(_) => {}
        Ok(plan) => {
            assert!(execute(&db, &job.id, &plan.id, true).is_err());
        }
    }
    assert_eq!(
        fs::read(std::path::Path::new(&job.current_path).join("database.db")).unwrap(),
        b"old valid copy"
    );
}

#[test]
#[ignore = "reproducible 10k-file throughput baseline; run explicitly"]
fn performance_10k_small_files() {
    let (_temp, db, job) = fixture();
    let start = std::time::Instant::now();
    for n in 0..10_000 {
        fs::write(
            std::path::Path::new(&job.source).join(format!("file-{n:05}")),
            b"small backup fixture",
        )
        .unwrap();
    }
    let fixture_time = start.elapsed();
    let start = std::time::Instant::now();
    approved_run(&db, &job);
    let initial = start.elapsed();
    let start = std::time::Instant::now();
    let p = preview(&db, &job.id).unwrap();
    assert_eq!(p.unchanged, 10_000);
    println!(
        "fixture={fixture_time:?}, initial={initial:?}, unchanged_preview={:?}",
        start.elapsed()
    );
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let sample=std::process::Command::new("powershell.exe").creation_flags(0x08000000)
            .args(["-NoProfile","-NonInteractive","-Command"])
            .arg(format!("$sample=Get-Process -Id {}; @{{cpuMs=$sample.TotalProcessorTime.TotalMilliseconds;peakWorkingSetBytes=$sample.PeakWorkingSet64;privateBytes=$sample.PrivateMemorySize64}} | ConvertTo-Json -Compress",std::process::id()))
            .output().unwrap();
        assert!(sample.status.success());
        println!(
            "process_metrics={}",
            String::from_utf8_lossy(&sample.stdout).trim()
        );
    }
}

fn edit_policy(
    db: &Connection,
    job: &SavedJob,
    archive_mode: &str,
    kind: &str,
    every: u32,
) -> SavedJob {
    save_job(
        db,
        JobDraft {
            advanced: Default::default(),
            secrets: None,
            id: Some(job.id.clone()),
            revision: Some(job.revision),
            name: job.name.clone(),
            source: job.source.clone(),
            destination: job.destination.clone(),
            kind: job.kind.clone(),
            schedule_automatic: true,
            wake_computer: false,
            deletion_guard_enabled: true,
            deletion_guard_min_files: 10,
            retention_days: job.retention_days,
            daily_minute: job.daily_minute,
            missed_run: job.missed_run.clone(),
            archive_mode: archive_mode.into(),
            schedule_kind: kind.into(),
            schedule_every: every,
        },
    )
    .unwrap()
}
#[test]
fn no_archive_requires_consent_and_preserves_existing_versions() {
    let (_temp, db, job) = fixture();
    let src = std::path::Path::new(&job.source).join("data.txt");
    let dst = std::path::Path::new(&job.current_path).join("data.txt");
    fs::write(&src, "first").unwrap();
    approved_run(&db, &job);
    fs::write(&src, "second longer version").unwrap();
    approved_run(&db, &job);
    let archives = list_archive(&db, &job.id, 0).unwrap();
    assert_eq!(archives.len(), 1);
    let job = edit_policy(&db, &job, "none", "hours", 3);
    assert!(!job.enabled);
    assert_eq!(job.schedule_every, 3);
    assert_eq!(job.schedule_kind, "hours");
    fs::write(&src, "third").unwrap();
    let plan = preview(&db, &job.id).unwrap();
    assert!(plan.archive_disabled);
    assert_eq!(
        execute(&db, &job.id, &plan.id, false).unwrap_err().code,
        "consent_required"
    );
    assert_eq!(fs::read_to_string(&dst).unwrap(), "second longer version");
    execute(&db, &job.id, &plan.id, true).unwrap();
    assert_eq!(fs::read_to_string(&dst).unwrap(), "third");
    fs::remove_file(&src).unwrap();
    approved_run(&db, &job);
    assert!(!dst.exists());
    let remaining = list_archive(&db, &job.id, 0).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, archives[0].id);
    assert_eq!(remaining[0].expires_at, archives[0].expires_at);
    restore(&db, &archives[0].id, false).unwrap();
    assert_eq!(fs::read_to_string(&dst).unwrap(), "first");
    let stats = dashboard(&db, Some(&job.id)).unwrap();
    assert_eq!(stats.archive_files, 1);
    assert_eq!(stats.last_run.unwrap().deleted, Some(1));
    assert!(stats
        .recent
        .iter()
        .any(|event| event.kind == "permanent_remove"));
}
#[test]
fn no_archive_still_rejects_wrong_destination_and_stale_preview() {
    let (_temp, db, job) = fixture();
    let job = edit_policy(&db, &job, "none", "minutes", 15);
    let src = std::path::Path::new(&job.source).join("a");
    let dst = std::path::Path::new(&job.current_path).join("a");
    fs::write(&src, "safe").unwrap();
    approved_run(&db, &job);
    fs::remove_file(&src).unwrap();
    let plan = preview(&db, &job.id).unwrap();
    fs::write(&src, "came back").unwrap();
    assert!(execute(&db, &job.id, &plan.id, true).is_err());
    assert_eq!(fs::read_to_string(&dst).unwrap(), "safe");
    let plan = preview(&db, &job.id).unwrap();
    fs::write(service_root(&job).join("owner"), "wrong").unwrap();
    assert!(execute(&db, &job.id, &plan.id, true).is_err());
    assert_eq!(fs::read_to_string(&dst).unwrap(), "safe");
}
#[test]
fn dashboard_uses_committed_counts_and_survives_new_previews() {
    let (_temp, db, job) = fixture();
    let source = std::path::Path::new(&job.source);
    let initial = dashboard(&db, None).unwrap();
    assert_eq!(initial.total_jobs, 1);
    assert!(initial.last_run.is_none());
    assert!(initial.recent.is_empty());
    fs::write(source.join("a"), "12345").unwrap();
    fs::write(source.join("b"), "xyz").unwrap();
    approved_run(&db, &job);
    let p = preview(&db, &job.id).unwrap();
    assert_eq!(p.unchanged, 2);
    let data = dashboard(&db, Some(&job.id)).unwrap();
    assert_eq!(data.successful_jobs, 1);
    assert_eq!(data.recent.len(), 2);
    let run = data.last_run.unwrap();
    assert_eq!(run.created, Some(2));
    assert_eq!(run.created_bytes, Some(8));
    assert_eq!(run.state, "succeeded");
    assert!(data.recent.iter().all(|event| event.state == "committed"));
    approved_run(&db, &job);
    let data = dashboard(&db, None).unwrap();
    assert_eq!(data.last_run.unwrap().unchanged, Some(2));
}
#[test]
fn schedule_bounds_and_calendar_days_cross_months_without_daily_runs() {
    use chrono::{NaiveDate, TimeZone};
    assert!(!valid_schedule("minutes", 0));
    assert!(valid_schedule("minutes", 44640));
    assert!(!valid_schedule("hours", 745));
    assert!(!valid_schedule("days", 366));
    let (_temp, db, job) = fixture();
    let mut job = edit_policy(&db, &job, "days", "days", 3);
    job.schedule_anchor = "2026-01-30".into();
    job.daily_minute = 180;
    let at = |day| {
        NaiveDate::parse_from_str(day, "%Y-%m-%d")
            .unwrap()
            .and_hms_opt(4, 0, 0)
            .unwrap()
    };
    assert!(!calendar_due(&job, at("2026-01-31"), None).unwrap());
    assert!(calendar_due(&job, at("2026-02-02"), None).unwrap());
    let last = chrono::Local
        .from_local_datetime(&at("2026-02-02"))
        .earliest()
        .unwrap()
        .timestamp();
    assert!(!calendar_due(&job, at("2026-02-02"), Some(last)).unwrap());
    job.missed_run = "run_once_when_available".into();
    assert!(calendar_due(&job, at("2026-02-04"), None).unwrap());
    assert!(!calendar_due(&job, at("2026-02-04"), Some(last)).unwrap());
    assert!(calendar_due(&job, at("2026-02-05"), Some(last)).unwrap());
    assert!(!calendar_due(&job, at("2026-01-29"), None).unwrap());
}

#[test]
fn bounded_file_journal_paginates_without_pruning_archives() {
    let (_temp, db, job) = fixture();
    fs::write(std::path::Path::new(&job.source).join("a"), "content").unwrap();
    approved_run(&db, &job);
    let run: String = db
        .query_row(
            "SELECT id FROM runs WHERE trigger_kind='manual' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    db.execute("INSERT INTO file_events(run_id,job_id,relative_path,kind,state,byte_count,happened_at) WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<1001) SELECT ?1,?2,'fixture-'||x,'create','committed',1,1 FROM n",rusqlite::params![run,job.id]).unwrap();
    approved_run(&db, &job);
    assert_eq!(
        db.query_row("SELECT count(*) FROM file_events", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1000
    );
    assert_eq!(file_history(&db, 0, false).unwrap().len(), 50);
    assert_eq!(file_history(&db, 950, false).unwrap().len(), 50);
    assert!(file_history(&db, 1000, false).unwrap().is_empty());
    assert!(file_history(&db, 0, true).unwrap().is_empty());
    assert!(file_history(&db, -1, false).is_err());
    assert_eq!(dashboard(&db, None).unwrap().recent.len(), 20);
}
#[test]
#[cfg(windows)]
fn no_archive_staging_failure_keeps_the_only_backup() {
    use std::os::windows::fs::OpenOptionsExt;
    let (_temp, db, job) = fixture();
    let job = edit_policy(&db, &job, "none", "daily", 1);
    let source = std::path::Path::new(&job.source).join("file");
    let dest = std::path::Path::new(&job.current_path).join("file");
    fs::write(&source, "previous valid copy").unwrap();
    approved_run(&db, &job);
    fs::write(&source, "new version").unwrap();
    let plan = preview(&db, &job.id).unwrap();
    let _locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&source)
        .unwrap();
    assert!(execute(&db, &job.id, &plan.id, true).is_err());
    assert_eq!(fs::read_to_string(&dest).unwrap(), "previous valid copy");
    assert!(list_archive(&db, &job.id, 0).unwrap().is_empty());
}

#[test]
fn failed_run_reports_copies_committed_before_the_failure() {
    let (_temp, db, job) = fixture();
    let source = std::path::Path::new(&job.source);
    fs::write(source.join("Z-update"), "old").unwrap();
    approved_run(&db, &job);
    fs::write(source.join("A-new"), "copied").unwrap();
    fs::write(source.join("Z-update"), "changed version").unwrap();
    let plan = preview(&db, &job.id).unwrap();
    db.execute_batch("CREATE TRIGGER inject_failure BEFORE UPDATE OF state ON operations WHEN NEW.relative_path='Z-update' AND NEW.state='staged' BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(execute(&db, &job.id, &plan.id, true).is_err());
    assert_eq!(
        fs::read_to_string(std::path::Path::new(&job.current_path).join("A-new")).unwrap(),
        "copied"
    );
    let data = dashboard(&db, Some(&job.id)).unwrap();
    let run = data.last_run.unwrap();
    assert_eq!(run.state, "failed");
    assert_eq!(run.created, Some(1));
    assert_eq!(run.created_bytes, Some(6));
    assert_eq!(run.updated, Some(0));
}

#[test]
fn new_jobs_write_directly_and_never_mirror_the_service_folder() {
    let (_temp, db, job) = fixture();
    assert_eq!(
        std::path::Path::new(&job.current_path),
        std::path::Path::new(&job.destination)
    );
    fs::write(std::path::Path::new(&job.source).join("file"), "payload").unwrap();
    approved_run(&db, &job);
    assert_eq!(
        fs::read_to_string(std::path::Path::new(&job.destination).join("file")).unwrap(),
        "payload"
    );
    let plan = preview(&db, &job.id).unwrap();
    assert_eq!(plan.deleted, 0);
    assert_eq!(plan.unchanged, 1);
    assert!(service_root(&job).join("owner").is_file());
    fs::create_dir(std::path::Path::new(&job.source).join(".nas-backup-manager")).unwrap();
    assert_eq!(
        preview(&db, &job.id).unwrap_err().code,
        "reserved_backup_path"
    );
    assert!(service_root(&job).join("owner").is_file());
}
#[test]
fn preexisting_destination_files_are_not_adopted_or_modified() {
    let (temp, db, job) = fixture();
    let destination = temp.path().join("unowned");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("keep"), "unrelated").unwrap();
    let draft = JobDraft {
        advanced: Default::default(),
        secrets: None,
        id: None,
        revision: None,
        name: "new".into(),
        source: job.source,
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
    };
    assert_eq!(
        save_job(&db, draft).unwrap_err().code,
        "destination_not_empty"
    );
    assert_eq!(
        fs::read_to_string(destination.join("keep")).unwrap(),
        "unrelated"
    );
    assert!(!destination.join(".nas-backup-manager").exists());
}
#[test]
fn legacy_jobs_keep_their_current_path_and_archives() {
    let (_temp, db, job) = fixture();
    // Recreate the old on-disk fixture, then load it through the new reader.
    let service = service_root(&job);
    let legacy = service.join(&job.id);
    fs::create_dir(&legacy).unwrap();
    for item in ["owner", "lock", "archive", "staging"] {
        fs::rename(service.join(item), legacy.join(item)).unwrap();
    }
    fs::create_dir(legacy.join("current")).unwrap();
    db.execute(
        "UPDATE jobs SET destination_layout='isolated' WHERE id=?1",
        [&job.id],
    )
    .unwrap();
    db.execute(
        "UPDATE job_runtime SET current_identity=?2,archive_identity=?3 WHERE job_id=?1",
        rusqlite::params![
            job.id,
            format!(
                "{:?}",
                file_id::get_file_id(legacy.join("current")).unwrap()
            ),
            format!(
                "{:?}",
                file_id::get_file_id(legacy.join("archive")).unwrap()
            )
        ],
    )
    .unwrap();
    let job = get_job(&db, &job.id).unwrap();
    assert_eq!(
        std::path::Path::new(&job.current_path),
        legacy.join("current")
    );
    fs::write(
        std::path::Path::new(&job.source).join("legacy.txt"),
        "old format",
    )
    .unwrap();
    approved_run(&db, &job);
    fs::remove_file(std::path::Path::new(&job.source).join("legacy.txt")).unwrap();
    approved_run(&db, &job);
    let versions = list_archive(&db, &job.id, 0).unwrap();
    assert_eq!(versions.len(), 1);
    restore(&db, &versions[0].id, false).unwrap();
    assert_eq!(
        fs::read_to_string(legacy.join("current/legacy.txt")).unwrap(),
        "old format"
    );
    assert!(!std::path::Path::new(&job.destination)
        .join("legacy.txt")
        .exists());
}

fn guard_draft(job: &SavedJob, enabled: bool, minimum: u32) -> JobDraft {
    JobDraft {
        advanced: Default::default(),
        secrets: None,
        id: Some(job.id.clone()),
        revision: Some(job.revision),
        name: job.name.clone(),
        source: job.source.clone(),
        destination: job.destination.clone(),
        kind: job.kind.clone(),
        retention_days: job.retention_days,
        archive_mode: job.archive_mode.clone(),
        daily_minute: job.daily_minute,
        missed_run: job.missed_run.clone(),
        schedule_kind: job.schedule_kind.clone(),
        schedule_every: job.schedule_every,
        schedule_automatic: true,
        wake_computer: false,
        deletion_guard_enabled: enabled,
        deletion_guard_min_files: minimum,
    }
}

#[test]
fn deletion_guard_uses_count_inclusively_without_percentage() {
    for (enabled, minimum, deleted, total, blocked, mode) in [
        (true, 10, 1, 2, false, "days"),
        (true, 10, 1, 2, false, "none"),
        (true, 10, 9, 10, false, "days"),
        (true, 10, 10, 11, true, "days"),
        (true, 10, 10, 100, true, "none"),
        (false, 1, 12, 12, false, "days"),
        (false, 1, 12, 12, false, "none"),
    ] {
        let (_temp, db, job) = fixture();
        let mut draft = guard_draft(&job, enabled, minimum);
        draft.archive_mode = mode.into();
        let job = save_job(&db, draft).unwrap();
        let source = std::path::Path::new(&job.source);
        let destination = std::path::Path::new(&job.current_path);
        for i in 0..total {
            fs::write(source.join(i.to_string()), "backup").unwrap();
        }
        approved_run(&db, &job);
        db.execute("UPDATE jobs SET schedule_enabled=1 WHERE id=?1", [&job.id])
            .unwrap();
        for i in 0..deleted {
            fs::remove_file(source.join(i.to_string())).unwrap();
        }
        assert_eq!(preview(&db, &job.id).unwrap().mass_deletion, blocked);
        let result = run_scheduled(&db, &job.id);
        if blocked {
            assert_eq!(result.unwrap_err().code, "mass_deletion");
            assert_eq!(fs::read_to_string(destination.join("0")).unwrap(), "backup");
            let error: String=db.query_row("SELECT error_code FROM runs WHERE trigger_kind='scheduled' ORDER BY rowid DESC LIMIT 1",[],|r|r.get(0)).unwrap();
            assert_eq!(error, "mass_deletion");
            assert!(list_archive(&db, &job.id, 0).unwrap().is_empty());
        } else {
            result.unwrap();
            assert!(!destination.join("0").exists());
            assert_eq!(
                list_archive(&db, &job.id, 0).unwrap().len(),
                if mode == "none" { 0 } else { deleted as usize }
            );
        }
    }
}

#[test]
fn guard_changes_invalidate_approval_and_disabled_guard_still_checks_source() {
    let (temp, db, job) = fixture();
    fs::write(std::path::Path::new(&job.source).join("keep"), "valid copy").unwrap();
    approved_run(&db, &job);
    let old = preview(&db, &job.id).unwrap();
    assert_eq!(
        save_job(&db, guard_draft(&job, false, 0)).unwrap_err().code,
        "invalid_job"
    );
    let mut draft = guard_draft(&job, false, 25);
    draft.wake_computer = true;
    let changed = save_job(&db, draft).unwrap();
    assert!(get_job(&db, &job.id).unwrap().wake_computer);
    assert!(!changed.enabled);
    assert_eq!(changed.revision, job.revision + 1);
    assert!(!get_job(&db, &job.id).unwrap().deletion_guard_enabled);
    assert_eq!(get_job(&db, &job.id).unwrap().deletion_guard_min_files, 25);
    assert!(execute(&db, &job.id, &old.id, true).is_err());
    approved_run(&db, &changed);
    db.execute("UPDATE jobs SET schedule_enabled=1 WHERE id=?1", [&job.id])
        .unwrap();
    fs::rename(&job.source, temp.path().join("offline")).unwrap();
    assert!(run_scheduled(&db, &job.id).is_err());
    assert_eq!(
        fs::read_to_string(std::path::Path::new(&job.current_path).join("keep")).unwrap(),
        "valid copy"
    );
}

#[test]
fn manual_mode_cannot_run_scheduled_and_recurrence_waits_for_approval() {
    let (_temp, db, job) = fixture();
    assert!(job.schedule_automatic && job.schedule_pending);
    assert!(!job.schedule_enabled);
    assert!(run_scheduled(&db, &job.id).is_err());
    let mut draft = guard_draft(&job, true, 10);
    draft.schedule_automatic = false;
    draft.wake_computer = true;
    let manual = save_job(&db, draft).unwrap();
    assert!(!manual.schedule_automatic && !manual.schedule_pending && !manual.schedule_enabled);
    assert!(!manual.wake_computer);
    fs::write(
        std::path::Path::new(&manual.source).join("manual.txt"),
        "manual only",
    )
    .unwrap();
    approved_run(&db, &manual);
    let manual = get_job(&db, &manual.id).unwrap();
    assert!(manual.enabled);
    assert!(!manual.schedule_pending && !manual.schedule_enabled);
    assert!(db
        .execute(
            "UPDATE jobs SET schedule_enabled=1 WHERE id=?1",
            [&manual.id]
        )
        .is_err());
    assert!(run_scheduled(&db, &manual.id).is_err());
    let mut draft = guard_draft(&manual, true, 10);
    draft.schedule_automatic = true;
    draft.schedule_kind = "minutes".into();
    draft.schedule_every = 10;
    let automatic = save_job(&db, draft).unwrap();
    assert!(automatic.schedule_automatic && automatic.schedule_pending);
    assert!(!automatic.enabled && !automatic.schedule_enabled);
    let plan = preview(&db, &automatic.id).unwrap();
    assert!(execute(&db, &automatic.id, &plan.id, false).is_err());
    assert!(!get_job(&db, &automatic.id).unwrap().schedule_enabled);
    execute(&db, &automatic.id, &plan.id, true).unwrap();
    assert!(get_job(&db, &automatic.id).unwrap().enabled);
    // Core never registers OS tasks; only the desktop consent handler may activate.
    assert!(!get_job(&db, &automatic.id).unwrap().schedule_enabled);
}

#[test]
fn cross_connection_progress_tracks_actual_bytes_and_cleans_stale_workers() {
    use nas_backup_core::activity;
    let (temp, memory, job) = fixture();
    let path = temp.path().join("progress.sqlite");
    // Fixture is recreated in a real local DB, without touching any user data.
    let db = nas_backup_storage::database::Database::open(&path).unwrap();
    let destination = temp.path().join("progress-destination");
    fs::create_dir(&destination).unwrap();
    let draft = JobDraft {
        advanced: Default::default(),
        secrets: None,
        id: None,
        revision: None,
        name: job.name,
        source: job.source,
        destination: destination.to_string_lossy().into(),
        kind: job.kind,
        schedule_automatic: false,
        wake_computer: false,
        deletion_guard_enabled: true,
        deletion_guard_min_files: 10,
        retention_days: 30,
        archive_mode: "days".into(),
        schedule_kind: "daily".into(),
        schedule_every: 1,
        daily_minute: 180,
        missed_run: "skip".into(),
    };
    drop(memory);
    let job = save_job(&db.connection, draft).unwrap();
    let reader = nas_backup_storage::database::Database::open(&path).unwrap();
    let guard = activity::begin(&db.connection, &job.id).unwrap();
    assert_eq!(activity::list(&reader.connection).unwrap().len(), 1);
    assert_eq!(
        activity::begin(&reader.connection, &job.id)
            .err()
            .unwrap()
            .code,
        "job_busy"
    );
    drop(guard);
    assert!(activity::list(&reader.connection).unwrap().is_empty());
    // Simulate a crash leaving a row but no lock owner.
    db.connection.execute("INSERT INTO job_activity(job_id,token,phase,updated_at) VALUES(?1,'stale','copying',1)",[&job.id]).unwrap();
    assert!(activity::list(&reader.connection).unwrap().is_empty());
    let content = vec![42u8; 8 * 1024 * 1024];
    fs::write(
        std::path::Path::new(&job.source).join("large.bin"),
        &content,
    )
    .unwrap();
    let plan = preview(&db.connection, &job.id).unwrap();
    let mut saw_verifying = false;
    let mut saw_verified = false;
    let mut final_bytes = 0;
    execute_progress(&db.connection, &job.id, &plan.id, true, |p| {
        let active = activity::list(&reader.connection).unwrap();
        assert_eq!(active.len(), 1);
        assert!(p.copied_bytes <= p.total_bytes);
        if p.phase == "verifying" {
            saw_verifying = true;
            if p.verified_bytes > 0 {
                saw_verified = true;
            }
            assert_eq!(active[0].verified_bytes, p.verified_bytes);
        }

        if p.phase == "flushing" {
            assert!(capacity(&reader.connection, &job.id).unwrap().0 > 0);
            assert_eq!(p.copied_bytes, content.len() as i64);
            assert_eq!(p.verified_bytes, 0);
            assert!(p.eta_seconds.is_none());
        }
        if p.phase == "finalizing" {
            assert_eq!(p.verified_bytes, content.len() as i64);
            final_bytes = p.copied_bytes;
            assert_eq!(p.completed, p.total);
        }
    })
    .unwrap();
    assert!(saw_verifying && saw_verified);
    assert_eq!(final_bytes, content.len() as i64);
    assert!(activity::list(&reader.connection).unwrap().is_empty());
    assert_eq!(
        fs::read(std::path::Path::new(&job.current_path).join("large.bin")).unwrap(),
        content
    );
}
#[test]
fn stale_preview_is_neutral_but_real_failures_still_count() {
    let (_temp, db, job) = fixture();
    let source = std::path::Path::new(&job.source).join("data.txt");
    fs::write(&source, "original").unwrap();
    let plan = preview(&db, &job.id).unwrap();
    fs::write(&source, "changed after preview").unwrap();
    assert_eq!(
        execute(&db, &job.id, &plan.id, true).unwrap_err().code,
        "stale_preview"
    );
    let summary = dashboard(&db, Some(&job.id)).unwrap();
    assert_eq!(summary.failed_jobs, 0);
    assert_eq!(
        summary.last_run.unwrap().error_code.as_deref(),
        Some("stale_preview")
    );
    assert!(history_filtered(&db, 0, true).unwrap().is_empty());
    assert!(nas_backup_core::activity::list(&db).unwrap().is_empty());
    db.execute("UPDATE runs SET error_code='source_unavailable',state='failed' WHERE trigger_kind='manual'",[]).unwrap();
    assert_eq!(dashboard(&db, Some(&job.id)).unwrap().failed_jobs, 1);
    assert_eq!(history_filtered(&db, 0, true).unwrap().len(), 1);
}

#[test]
fn startup_schedule_round_trips_without_changing_existing_intervals() {
    let (_temp, db, job) = fixture();
    assert_eq!(get_job(&db, &job.id).unwrap().schedule_kind, "daily");
    assert!(valid_schedule("startup", 1));
    assert!(!valid_schedule("startup", 2));
    let destination = _temp.path().join("new-startup");
    fs::create_dir(&destination).unwrap();
    let created = save_job(
        &db,
        JobDraft {
            advanced: Default::default(),
            secrets: None,
            id: None,
            revision: None,
            name: "Startup".into(),
            source: job.source.clone(),
            destination: destination.to_string_lossy().into(),
            kind: "local".into(),
            schedule_automatic: true,
            wake_computer: true,
            deletion_guard_enabled: true,
            deletion_guard_min_files: 10,
            retention_days: 30,
            archive_mode: "days".into(),
            schedule_kind: "startup".into(),
            schedule_every: 1,
            daily_minute: 180,
            missed_run: "skip".into(),
        },
    )
    .unwrap();
    assert_eq!(get_job(&db, &created.id).unwrap().schedule_kind, "startup");
    assert!(!created.wake_computer);
    let startup = edit_policy(&db, &job, "days", "startup", 1);
    assert_eq!(get_job(&db, &startup.id).unwrap().schedule_kind, "startup");
    assert!(startup.schedule_automatic && startup.schedule_pending && !startup.schedule_enabled);
    assert!(!startup.wake_computer);
    assert_eq!(
        db.query_row(
            "SELECT schedule_kind,schedule_on_login FROM jobs WHERE id=?1",
            [&job.id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?))
        )
        .unwrap(),
        ("daily".into(), true)
    );
    let regular = edit_policy(&db, &startup, "days", "minutes", 3);
    assert_eq!(regular.schedule_kind, "minutes");
    assert!(!db
        .query_row(
            "SELECT schedule_on_login FROM jobs WHERE id=?1",
            [&job.id],
            |r| r.get::<_, bool>(0)
        )
        .unwrap());
}
