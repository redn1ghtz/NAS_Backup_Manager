use nas_backup_core::{
    advanced::{self, Options},
    backup::*,
};
use nas_backup_storage::database::Database;
use std::{fs, path::Path, time::Duration};
fn fixture() -> (tempfile::TempDir, Database, SavedJob) {
    let t = tempfile::tempdir().unwrap();
    let s = t.path().join("source");
    let d = t.path().join("copy");
    fs::create_dir(&s).unwrap();
    fs::create_dir(&d).unwrap();
    let db = Database::open(&t.path().join("state.sqlite")).unwrap();
    let job = save_job(
        &db.connection,
        JobDraft {
            id: None,
            revision: None,
            name: "Safety fixture".into(),
            source: s.to_string_lossy().into(),
            destination: d.to_string_lossy().into(),
            kind: "local".into(),
            schedule_automatic: false,
            wake_computer: false,
            deletion_guard_enabled: true,
            deletion_guard_min_files: 10,
            retention_days: 30,
            archive_mode: "days".into(),
            schedule_kind: "daily".into(),
            schedule_every: 1,
            daily_minute: 0,
            missed_run: "skip".into(),
            advanced: Default::default(),
            secrets: None,
        },
    )
    .unwrap();
    (t, db, job)
}
fn run(db: &Database, job: &SavedJob) {
    let p = preview(&db.connection, &job.id).unwrap();
    execute(&db.connection, &job.id, &p.id, true).unwrap();
}
#[test]
fn detects_silent_same_size_same_mtime_corruption_and_keeps_source() {
    let (_t, db, j) = fixture();
    let s = Path::new(&j.source).join("a");
    let d = Path::new(&j.current_path).join("a");
    fs::write(&s, b"valid").unwrap();
    run(&db, &j);
    let stamp = filetime::FileTime::from_last_modification_time(&fs::metadata(&d).unwrap());
    fs::write(&d, b"wrong").unwrap();
    filetime::set_file_mtime(&d, stamp).unwrap();
    assert_eq!(preview(&db.connection, &j.id).unwrap().unchanged, 1);
    assert_eq!(
        verify_integrity(&db.connection, &j.id).unwrap_err().code,
        "integrity_failed"
    );
    assert_eq!(fs::read(&s).unwrap(), b"valid");
    assert_eq!(fs::read(&d).unwrap(), b"wrong");
    fs::write(&s, b"new source").unwrap();
    let p = preview(&db.connection, &j.id).unwrap();
    assert_eq!(
        execute(&db.connection, &j.id, &p.id, true)
            .unwrap_err()
            .code,
        "integrity_failed"
    );
    assert_eq!(fs::read(&d).unwrap(), b"wrong");
}
#[test]
fn verification_covers_archives_and_never_hashes_on_default_preview() {
    let (_t, db, j) = fixture();
    let s = Path::new(&j.source).join("a");
    fs::write(&s, b"one").unwrap();
    run(&db, &j);
    fs::write(&s, b"two2").unwrap();
    run(&db, &j);
    assert_eq!(
        verify_integrity(&db.connection, &j.id)
            .unwrap()
            .checked_files,
        2
    );
    let stored: String = db
        .connection
        .query_row(
            "SELECT stored_relative_path FROM archive_versions WHERE job_id=?1",
            [&j.id],
            |r| r.get(0),
        )
        .unwrap();
    let archive = Path::new(&j.destination)
        .join(".nas-backup-manager/archive")
        .join(stored);
    fs::write(&archive, b"bad").unwrap();
    assert_eq!(
        verify_integrity(&db.connection, &j.id).unwrap_err().code,
        "integrity_failed"
    );
    assert!(archive.exists());
    assert!(!advanced::verify_due(&db.connection, &j).unwrap());
}

#[test]
fn integrity_progress_counts_bytes_between_files_and_zero_disables_limit() {
    let (_t, db, mut j) = fixture();
    fs::write(
        Path::new(&j.source).join("large.bin"),
        vec![42u8; 1024 * 1024],
    )
    .unwrap();
    run(&db, &j);
    j.advanced.verify_mib_per_second = 1;
    let guard = nas_backup_core::activity::begin(&db.connection, &j.id).unwrap();
    let mut intermediate = false;
    let mut previous = 0;
    advanced::verify_existing(&db.connection, &j, &mut || {
        let live = nas_backup_core::activity::list(&db.connection)?;
        let p = &live[0];
        assert_eq!(p.phase, "integrity");
        assert_eq!(p.total, 1);
        assert_eq!(p.total_bytes, 1024 * 1024);
        assert!(p.verified_bytes >= previous);
        previous = p.verified_bytes;
        if p.verified_bytes > 0 && p.verified_bytes < p.total_bytes {
            intermediate = true;
        }
        Ok(())
    })
    .unwrap();
    assert!(intermediate);
    let live = nas_backup_core::activity::list(&db.connection).unwrap();
    assert_eq!(live[0].verified_bytes, 1024 * 1024);
    assert_eq!(live[0].completed, 1);
    j.advanced.verify_mib_per_second = 0;
    advanced::validate(&j.advanced).unwrap();
    assert_eq!(
        advanced::verify_existing(&db.connection, &j, &mut || Ok(()))
            .unwrap()
            .checked_files,
        1
    );
    assert_eq!(
        advanced::verify_existing(&db.connection, &j, &mut || Err(Failure {
            code: "cancelled".into(),
            detail: String::new()
        }))
        .unwrap_err()
        .code,
        "cancelled"
    );
    drop(guard);
    assert!(nas_backup_core::activity::list(&db.connection)
        .unwrap()
        .is_empty());
}
#[test]
fn throttle_supports_midnight_and_backoff_is_bounded() {
    assert!(advanced::bandwidth_active(1320, 420, 0));
    assert!(advanced::bandwidth_active(1320, 420, 1320));
    assert!(!advanced::bandwidth_active(1320, 420, 420));
    assert!(advanced::bandwidth_active(0, 0, 900));
    let o = Options::default();
    assert_eq!(advanced::retry_delay(&o, 0), Duration::from_secs(2));
    assert_eq!(advanced::retry_delay(&o, 2), Duration::from_secs(8));
    assert_eq!(advanced::retry_delay(&o, 32), Duration::from_secs(60));
    assert!(!advanced::transient(&std::io::Error::from(
        std::io::ErrorKind::PermissionDenied
    )));
    assert!(advanced::transient(&std::io::Error::from(
        std::io::ErrorKind::ConnectionReset
    )));
}
#[test]
fn retry_copy_never_overwrites_existing_staging_and_cancellation_is_bounded() {
    let t = tempfile::tempdir().unwrap();
    let s = t.path().join("source");
    let d = t.path().join("staging");
    fs::write(&s, vec![7; 1024 * 1024]).unwrap();
    fs::write(&d, "keep").unwrap();
    assert!(advanced::copy_resume(&s, &d, &Options::default(), &mut |_, _, _| Ok(())).is_err());
    assert_eq!(fs::read(&d).unwrap(), b"keep");
    let o = Options {
        bandwidth_kib: 1,
        bandwidth_start_minute: 0,
        bandwidth_end_minute: 0,
        ..Default::default()
    };
    let target = t.path().join("cancelled");
    let mut count = 0;
    let result = advanced::copy_resume(&s, &target, &o, &mut |_, _, _| {
        count += 1;
        if count > 2 {
            Err(Failure {
                code: "cancelled".into(),
                detail: String::new(),
            })
        } else {
            Ok(())
        }
    });
    assert_eq!(result.unwrap_err().code, "cancelled");
}
#[test]
fn remote_paths_and_serialization_reject_injection_and_secrets() {
    for p in [
        "remote:",
        "remote:/",
        "remote:path/../x",
        ":local:C:/x",
        "remote:path\n--delete",
        "remote:path\\x",
        "remote:a//b",
    ] {
        assert!(advanced::remote_parts(p).is_err(), "{p}");
    }
    assert!(advanced::remote_parts("backup:Backup/Documents").is_ok());
    let o = Options::default();
    let json = serde_json::to_string(&o).unwrap();
    assert!(!json.contains("password"));
    let secret = advanced::Secrets {
        telegram_token: "123:secret".into(),
        ..Default::default()
    };
    assert!(!format!("{secret:?}").contains("123:secret"));
}
#[test]
fn migration_roundtrip_does_not_activate_existing_jobs() {
    let (_t, db, j) = fixture();
    assert_eq!(db.schema_version().unwrap(), 13);
    let actual = get_job(&db.connection, &j.id).unwrap();
    assert_eq!(actual.advanced, Options::default());
    assert!(!actual.enabled);
    assert!(!actual.schedule_enabled);
}
#[test]
#[cfg(windows)]
fn dpapi_roundtrip_contains_no_plaintext_and_tampering_fails() {
    let t = tempfile::tempdir().unwrap();
    let p = t.path().join("vault.dpapi");
    let value = b"fixture-only:Telegram-SMTP-OAuth-PRIVATE-KEY";
    nas_backup_storage::vault::save_new(&p, value).unwrap();
    let encrypted = fs::read(&p).unwrap();
    assert!(!encrypted.windows(value.len()).any(|part| part == value));
    assert_eq!(nas_backup_storage::vault::load(&p).unwrap(), value);
    assert!(nas_backup_storage::vault::save_new(&p, b"overwrite").is_err());
    fs::write(&p, b"tampered").unwrap();
    assert!(nas_backup_storage::vault::load(&p).is_err());
}

#[test]
#[ignore = "requires explicit disposable age tool paths"]
fn age_backup_roundtrip_changes_and_safe_decryption() {
    use std::process::Command;
    let (t, db, j) = fixture();
    let exe = std::env::var("NBM_TEST_AGE").unwrap();
    let keygen = std::env::var("NBM_TEST_AGE_KEYGEN").unwrap();
    let identity = t.path().join("fixture.key");
    let (recipient, generated) =
        nas_backup_core::remote::generate_identity(&keygen, &identity).unwrap();
    assert_eq!(fs::read_to_string(&identity).unwrap(), generated);
    assert!(nas_backup_core::remote::generate_identity(&keygen, &identity).is_err());
    assert_eq!(fs::read_to_string(&identity).unwrap(), generated);
    let derived = Command::new(&keygen)
        .arg("-y")
        .arg(&identity)
        .output()
        .unwrap();
    assert!(derived.status.success());
    assert_eq!(String::from_utf8(derived.stdout).unwrap().trim(), recipient);
    let mut options = Options {
        encryption_recipient: recipient,
        age_executable: exe,
        ..Default::default()
    };
    advanced::save_secrets(
        &db.connection,
        &mut options,
        None,
        Some(&advanced::Secrets {
            age_identity: fs::read_to_string(identity).unwrap(),
            ..Default::default()
        }),
    )
    .unwrap();
    db.connection
        .execute(
            "UPDATE jobs SET advanced_json=?2 WHERE id=?1",
            rusqlite::params![j.id, serde_json::to_string(&options).unwrap()],
        )
        .unwrap();
    let j = get_job(&db.connection, &j.id).unwrap();
    let s = Path::new(&j.source).join("secret.txt");
    let d = Path::new(&j.current_path).join("secret.txt");
    fs::write(&s, b"disposable secret original").unwrap();
    run(&db, &j);
    assert!(fs::read(&d).unwrap().starts_with(b"age-encryption.org/v1"));
    assert!(!String::from_utf8_lossy(&fs::read(&d).unwrap()).contains("disposable secret original"));
    assert_eq!(preview(&db.connection, &j.id).unwrap().unchanged, 1);
    let export = t.path().join("export");
    fs::create_dir(&export).unwrap();
    decrypt_file(&db.connection, &j.id, "secret.txt", &export).unwrap();
    assert_eq!(
        fs::read(export.join("secret.txt")).unwrap(),
        b"disposable secret original"
    );
    assert!(decrypt_file(&db.connection, &j.id, "secret.txt", &export).is_err());
    fs::write(&s, b"new secret").unwrap();
    run(&db, &j);
    assert_eq!(
        verify_integrity(&db.connection, &j.id)
            .unwrap()
            .checked_files,
        2
    );
    fs::remove_file(&s).unwrap();
    run(&db, &j);
    let versions = list_archive(&db.connection, &j.id, 0).unwrap();
    assert_eq!(versions.len(), 2);
    restore(&db.connection, &versions[0].id, false).unwrap();
    assert!(fs::read(&d).unwrap().starts_with(b"age-encryption.org/v1"));
    let vault = serde_json::to_string(&get_job(&db.connection, &j.id).unwrap()).unwrap();
    assert!(!vault.contains("AGE-SECRET-KEY-"));
    assert!(versions.iter().all(|v| v.encrypted));
    remove_job(&db.connection, &j.id).unwrap();
    let exported = t.path().join("archive-export");
    fs::create_dir(&exported).unwrap();
    decrypt_archive(&db.connection, &versions[0].id, &exported).unwrap();
    assert_eq!(
        fs::read(exported.join("secret.txt")).unwrap(),
        b"new secret"
    );
}

#[test]
#[cfg(windows)]
#[ignore = "requires explicit rclone path; only local disposable alias remote is used"]
fn cloud_copy_ownership_archive_quota_and_checksum_roundtrip() {
    use std::process::Command;
    let (t, db, j) = fixture();
    let tool = std::env::var("NBM_TEST_RCLONE").unwrap();
    let cloud = t.path().join("cloud");
    fs::create_dir(&cloud).unwrap();
    let config = t.path().join("rclone.conf");
    fs::write(
        &config,
        format!("[fixture]\ntype = alias\nremote = {}\n", cloud.display()),
    )
    .unwrap();
    let output=Command::new(&tool).arg("--config").arg(&config).args(["--password-command","powershell.exe -NoProfile -NonInteractive -Command Write-Output disposable-fixture-pass","config","encryption","set"]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut o = Options {
        cloud_mode: "replica".into(),
        cloud_remote: "fixture:copies".into(),
        rclone_executable: tool,
        rclone_config: config.to_string_lossy().into(),
        ..Default::default()
    };
    advanced::validate(&o).unwrap();
    advanced::save_secrets(
        &db.connection,
        &mut o,
        None,
        Some(&advanced::Secrets {
            rclone_password: "disposable-fixture-pass".into(),
            ..Default::default()
        }),
    )
    .unwrap();
    db.connection
        .execute(
            "UPDATE jobs SET advanced_json=?2 WHERE id=?1",
            rusqlite::params![j.id, serde_json::to_string(&o).unwrap()],
        )
        .unwrap();
    let j = get_job(&db.connection, &j.id).unwrap();
    let s = Path::new(&j.source).join("a");
    fs::write(&s, b"first").unwrap();
    run(&db, &j);
    assert_eq!(fs::read(cloud.join("copies/a")).unwrap(), b"first");
    assert_eq!(
        verify_integrity(&db.connection, &j.id)
            .unwrap()
            .checked_files,
        1
    );
    let _quota = nas_backup_core::remote::capacity(&db.connection, &j, false).unwrap();
    fs::write(&s, b"second version").unwrap();
    run(&db, &j);
    assert_eq!(fs::read(cloud.join("copies/a")).unwrap(), b"second version");
    assert_eq!(
        verify_integrity(&db.connection, &j.id)
            .unwrap()
            .checked_files,
        2
    );
    fs::write(cloud.join("copies/a"), b"corrupt-cloud").unwrap();
    assert_eq!(
        verify_integrity(&db.connection, &j.id).unwrap_err().code,
        "integrity_failed"
    );
    fs::write(
        cloud.join("copies/.nas-backup-manager/cloud-owner"),
        b"foreign",
    )
    .unwrap();
    let p = preview(&db.connection, &j.id).unwrap();
    assert_eq!(
        execute(&db.connection, &j.id, &p.id, true)
            .unwrap_err()
            .code,
        "ownership_mismatch"
    );
    assert_eq!(fs::read(cloud.join("copies/a")).unwrap(), b"corrupt-cloud");
    assert!(!fs::read(db.connection.path().unwrap())
        .unwrap()
        .windows(b"disposable-fixture-pass".len())
        .any(|part| part == b"disposable-fixture-pass"));
}
