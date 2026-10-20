#![cfg(windows)]
//! Explicit opt-in only: disposable source/destination/database and a unique OS task.
use nas_backup_core::backup::*;
use nas_backup_scheduler::registration;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

struct Registered(SavedJob);
impl Drop for Registered {
    fn drop(&mut self) {
        let _ = registration::unregister(&self.0);
    }
}

#[test]
#[ignore = "requires explicit OS scheduler access and built CLI"]
fn headless_task_runs_with_no_gui() {
    let cli = PathBuf::from(
        std::env::var_os("NAS_BACKUP_TEST_CLI")
            .expect("Set NAS_BACKUP_TEST_CLI to the built absolute CLI executable"),
    );
    assert!(cli.is_absolute() && cli.is_file());
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&destination).unwrap();
    fs::write(source.join("fixture.txt"), "task scheduler fixture").unwrap();
    let state = temp.path().join("state.sqlite");
    let db = nas_backup_storage::database::Database::open(&state).unwrap();
    db.set_notifications_enabled(false).unwrap();
    let job = save_job(
        &db.connection,
        JobDraft {
            id: None,
            revision: None,
            name: "Disposable scheduler test".into(),
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
            daily_minute: 0,
            missed_run: "skip".into(),
        },
    )
    .unwrap();
    let preview = preview(&db.connection, &job.id).unwrap();
    execute(&db.connection, &job.id, &preview.id, true).unwrap();
    fs::write(source.join("new-after-gui-closed.txt"), "headless content").unwrap();
    db.connection
        .execute("UPDATE jobs SET schedule_enabled=1 WHERE id=?1", [&job.id])
        .unwrap();
    registration::register(&job, &cli, &state).unwrap();
    let registration = Registered(job.clone());
    let next = registration::next_run(&job).unwrap();
    assert!(
        next.contains('T') && next.ends_with('Z'),
        "Expected actual UTC next-run time from Windows Task Scheduler"
    );
    let result = Command::new("schtasks.exe")
        .args(["/Run", "/TN", &format!("NASBackupManager-{}", job.id)])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "Task Scheduler rejected the disposable task invocation"
    );
    let start = Instant::now();
    loop {
        let finished:i64=db.connection.query_row("SELECT count(*) FROM runs WHERE job_id=?1 AND trigger_kind='scheduled' AND state='succeeded'",[&job.id],|r|r.get(0)).unwrap();
        if finished > 0 {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(45),
            "Headless task did not finish. Check disposable task history/permissions."
        );
        std::thread::sleep(Duration::from_millis(250));
    }
    assert_eq!(
        fs::read(PathBuf::from(&job.current_path).join("new-after-gui-closed.txt")).unwrap(),
        b"headless content"
    );
    println!(
        "Verified GUI-closed scheduled headless execution in {:?}",
        start.elapsed()
    );
    // A second direct headless run waits for native presentation to finish too.
    let direct = Command::new(&cli)
        .args(["--state", state.to_str().unwrap(), "run-scheduled", &job.id])
        .status()
        .unwrap();
    assert!(direct.success());
    let notification_failures: i64 = db
        .connection
        .query_row(
            "SELECT count(*) FROM audit_events WHERE kind='notification' AND state='failed'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(notification_failures, 0, "Native tray host failed");
    assert!(nas_backup_core::activity::list(&db.connection)
        .unwrap()
        .is_empty());
    assert!(
        !state.parent().unwrap().join("native").exists(),
        "Headless execution must not unpack executables into settings"
    );
    drop(registration);
    for (kind, every) in [("minutes", 15), ("hours", 2), ("days", 3), ("startup", 1)] {
        let mut variant = job.clone();
        variant.schedule_kind = kind.into();
        variant.schedule_every = every;
        variant.wake_computer = kind == "hours";
        registration::register(&variant, &cli, &state).unwrap();
        let guard = Registered(variant.clone());
        let settings=Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-Command","$ErrorActionPreference='Stop'; $t=Get-ScheduledTask -TaskName $env:NAS_BACKUP_TEST_TASK; Write-Output ($t.Settings.WakeToRun.ToString()+','+$t.Settings.Hidden.ToString())"]).env("NAS_BACKUP_TEST_TASK",format!("NASBackupManager-{}",variant.id)).output().unwrap();
        assert!(settings.status.success());
        assert_eq!(
            String::from_utf8(settings.stdout).unwrap().trim(),
            if variant.wake_computer {
                "True,False"
            } else {
                "False,False"
            }
        );
        if kind == "startup" {
            let check=Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-Command","$ErrorActionPreference='Stop'; $t=Get-ScheduledTask -TaskName $env:NAS_BACKUP_TEST_TASK; if($t.Triggers[0].CimClass.CimClassName -ne 'MSFT_TaskLogonTrigger' -or $t.Triggers[0].Delay -ne 'PT30S' -or [string]::IsNullOrEmpty($t.Triggers[0].UserId)){throw 'Invalid logon trigger'}; Write-Output 'user logon + 30s'"]).env("NAS_BACKUP_TEST_TASK",format!("NASBackupManager-{}",variant.id)).output().unwrap();
            assert!(
                check.status.success(),
                "Native logon trigger verification failed"
            );
            assert!(registration::next_run(&variant).is_err());
            println!("Verified native user-logon trigger with 30s delay");
            drop(guard);
            continue;
        }
        let next = registration::next_run(&variant).unwrap();
        let next = chrono::DateTime::parse_from_rfc3339(&next).unwrap();
        assert!(
            next.timestamp() > chrono::Utc::now().timestamp(),
            "Next {kind} trigger must be in the future"
        );
        println!("Registered {kind}/{every}; next actual trigger: {next}");
        drop(guard);
    }
    let query = Command::new("schtasks.exe")
        .args(["/Query", "/TN", &format!("NASBackupManager-{}", job.id)])
        .output()
        .unwrap();
    assert!(
        !query.status.success(),
        "Disposable scheduled task was not removed"
    );
}
