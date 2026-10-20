//! OS wakeups only; never launches or unregisters tasks during tests.
use nas_backup_core::{
    backup::{valid_schedule, SavedJob},
    model::JobId,
};
use std::{fs, path::Path, process::Command};

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn quote(value: &Path) -> Result<String, String> {
    let value = value.to_str().ok_or("non_unicode_path")?;
    if value.contains(['\n', '\r', '"']) {
        return Err("invalid_path".into());
    }
    Ok(format!("\"{value}\""))
}
fn run(command: &mut Command) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let result = command.output().map_err(|_| "scheduler_unavailable")?;
    if !result.status.success() {
        if cfg!(debug_assertions) {
            eprintln!(
                "OS scheduler status: {:?}; {}",
                result.status.code(),
                String::from_utf8_lossy(&result.stderr)
            );
        }
        return Err("scheduler_registration_failed".into());
    }
    Ok(())
}
pub fn windows_xml(job: &SavedJob, cli: &Path, state: &Path, user: &str) -> Result<String, String> {
    JobId::parse(&job.id).map_err(|_| "invalid_job")?;
    if !job.schedule_automatic
        || !valid_schedule(&job.schedule_kind, job.schedule_every)
        || job.daily_minute >= 1440
    {
        return Err("invalid_job".into());
    }
    let date = chrono::NaiveDate::parse_from_str(&job.schedule_anchor, "%Y-%m-%d")
        .map_err(|_| "invalid_job")?;
    let trigger = if job.schedule_kind == "startup" {
        format!("<LogonTrigger><Enabled>true</Enabled><UserId>{}</UserId><Delay>PT30S</Delay></LogonTrigger>",xml(user))
    } else if matches!(job.schedule_kind.as_str(), "minutes" | "hours") {
        let minutes = job.schedule_every * if job.schedule_kind == "hours" { 60 } else { 1 };
        let start =
            (chrono::Local::now() + chrono::Duration::minutes(1)).format("%Y-%m-%dT%H:%M:%S");
        format!("<TimeTrigger><Repetition><Interval>PT{minutes}M</Interval><StopAtDurationEnd>false</StopAtDurationEnd></Repetition><StartBoundary>{start}</StartBoundary><Enabled>true</Enabled></TimeTrigger>")
    } else {
        let days = if job.schedule_kind == "days" {
            job.schedule_every
        } else {
            1
        };
        format!("<CalendarTrigger><StartBoundary>{date}T{:02}:{:02}:00</StartBoundary><Enabled>true</Enabled><ScheduleByDay><DaysInterval>{days}</DaysInterval></ScheduleByDay></CalendarTrigger>",job.daily_minute/60,job.daily_minute%60)
    };
    let arguments = format!("--state {} run-scheduled {}", quote(state)?, job.id);
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
<Triggers>{trigger}</Triggers>
<Principals><Principal id="Author"><UserId>{}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
<Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><StartWhenAvailable>{}</StartWhenAvailable><AllowStartOnDemand>true</AllowStartOnDemand><Enabled>true</Enabled><Hidden>false</Hidden><WakeToRun>{}</WakeToRun><ExecutionTimeLimit>PT0S</ExecutionTimeLimit></Settings>
<Actions Context="Author"><Exec><Command>{}</Command><Arguments>{}</Arguments></Exec></Actions></Task>"#,
        xml(user),
        job.missed_run == "run_once_when_available",
        job.wake_computer,
        xml(cli.to_str().ok_or("invalid_path")?),
        xml(&arguments)
    ))
}
pub fn register(job: &SavedJob, cli: &Path, state: &Path) -> Result<(), String> {
    if job.wake_computer && !cfg!(windows) {
        return Err("wake_unsupported".into());
    }
    if !cli.is_absolute() || !cli.is_file() || !state.is_absolute() {
        return Err("scheduler_cli_missing".into());
    }
    JobId::parse(&job.id).map_err(|_| "invalid_job")?;
    if !job.schedule_automatic || !valid_schedule(&job.schedule_kind, job.schedule_every) {
        return Err("invalid_job".into());
    }
    #[cfg(windows)]
    {
        let user = format!(
            "{}\\{}",
            std::env::var("USERDOMAIN").map_err(|_| "scheduler_user")?,
            std::env::var("USERNAME").map_err(|_| "scheduler_user")?
        );
        let folder = state.parent().ok_or("invalid_path")?.join("schedules");
        fs::create_dir_all(&folder).map_err(|_| "scheduler_state")?;
        let path = folder.join(format!("{}.xml", job.id));
        let content = windows_xml(job, cli, state, &user)?;
        let mut bytes = vec![0xff, 0xfe];
        for unit in content.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        fs::write(&path, bytes).map_err(|_| "scheduler_state")?;
        run(Command::new("schtasks.exe")
            .args([
                "/Create",
                "/F",
                "/TN",
                &format!("NASBackupManager-{}", job.id),
                "/XML",
            ])
            .arg(path))
    }
    #[cfg(target_os = "linux")]
    {
        let folder = home()?.join(".config/systemd/user");
        fs::create_dir_all(&folder).map_err(|_| "scheduler_state")?;
        let name = format!("nas-backup-{}", job.id);
        let escape = |s: String| s.replace('%', "%%").replace('\\', "\\\\");
        let exec = format!(
            "{} --state {} run-scheduled {}",
            escape(quote(cli)?),
            escape(quote(state)?),
            job.id
        );
        fs::write(folder.join(format!("{name}.service")),format!("[Unit]\nDescription=NAS Backup Manager\n[Service]\nType=oneshot\nExecStart={exec}\n")).map_err(|_|"scheduler_state")?;
        let timer = if job.schedule_kind == "startup" {
            "OnStartupSec=30s\n".into()
        } else if matches!(job.schedule_kind.as_str(), "minutes" | "hours") {
            let minutes = job.schedule_every * if job.schedule_kind == "hours" { 60 } else { 1 };
            format!("OnActiveSec=1min\nOnUnitActiveSec={minutes}min\n")
        } else {
            // The headless calendar gate skips non-due days without touching backup folders.
            format!(
                "OnCalendar=*-*-* {:02}:{:02}:00\nPersistent={}\n",
                job.daily_minute / 60,
                job.daily_minute % 60,
                job.missed_run == "run_once_when_available"
            )
        };
        fs::write(folder.join(format!("{name}.timer")),format!("[Unit]\nDescription=NAS Backup Manager timer\n[Timer]\n{timer}AccuracySec=1s\n[Install]\nWantedBy=timers.target\n")).map_err(|_|"scheduler_state")?;
        run(Command::new("systemctl").args(["--user", "daemon-reload"]))?;
        run(Command::new("systemctl").args(["--user", "enable", "--now", &format!("{name}.timer")]))
    }
    #[cfg(target_os = "macos")]
    {
        let folder = home()?.join("Library/LaunchAgents");
        fs::create_dir_all(&folder).map_err(|_| "scheduler_state")?;
        let path = folder.join(format!("org.nasbackupmanager.{}.plist", job.id));
        let trigger = if job.schedule_kind == "startup" {
            "<key>RunAtLoad</key><true/>".into()
        } else if matches!(job.schedule_kind.as_str(), "minutes" | "hours") {
            let seconds = job.schedule_every
                * if job.schedule_kind == "hours" {
                    3600
                } else {
                    60
                };
            format!("<key>StartInterval</key><integer>{seconds}</integer>")
        } else {
            format!("<key>StartCalendarInterval</key><dict><key>Hour</key><integer>{}</integer><key>Minute</key><integer>{}</integer></dict>",job.daily_minute/60,job.daily_minute%60)
        };
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>Label</key><string>org.nasbackupmanager.{}</string><key>ProgramArguments</key><array><string>{}</string><string>--state</string><string>{}</string><string>run-scheduled</string><string>{}</string></array>{trigger}<key>ProcessType</key><string>Background</string></dict></plist>"#,
            job.id,
            xml(cli.to_str().ok_or("invalid_path")?),
            xml(state.to_str().ok_or("invalid_path")?),
            job.id
        );
        fs::write(&path, plist).map_err(|_| "scheduler_state")?;
        run(Command::new("launchctl").arg("load").arg(path))
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        Err("scheduler_unavailable".into())
    }
}
#[cfg(not(windows))]
fn home() -> Result<std::path::PathBuf, String> {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .ok_or("scheduler_home".into())
}
pub fn unregister(job: &SavedJob) -> Result<(), String> {
    JobId::parse(&job.id).map_err(|_| "invalid_job")?;
    #[cfg(windows)]
    {
        run(Command::new("schtasks.exe").args([
            "/Delete",
            "/F",
            "/TN",
            &format!("NASBackupManager-{}", job.id),
        ]))
    }
    #[cfg(target_os = "linux")]
    {
        run(Command::new("systemctl").args([
            "--user",
            "disable",
            "--now",
            &format!("nas-backup-{}.timer", job.id),
        ]))
    }
    #[cfg(target_os = "macos")]
    {
        run(Command::new("launchctl")
            .arg("unload")
            .arg(home()?.join(format!(
                "Library/LaunchAgents/org.nasbackupmanager.{}.plist",
                job.id
            ))))
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        Err("scheduler_unavailable".into())
    }
}

pub fn next_run(job: &SavedJob) -> Result<String, String> {
    if job.schedule_kind == "startup" {
        return Err("next_run_unavailable".into());
    }
    JobId::parse(&job.id).map_err(|_| "invalid_job")?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let result=Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-Command","$ErrorActionPreference='Stop'; (Get-ScheduledTaskInfo -TaskName $env:NAS_BACKUP_TASK).NextRunTime.ToUniversalTime().ToString('o')"]).env("NAS_BACKUP_TASK",format!("NASBackupManager-{}",job.id)).creation_flags(0x08000000).output().map_err(|_|"scheduler_unavailable")?;
        if !result.status.success() {
            return Err("scheduler_registration_failed".into());
        }
        let value = String::from_utf8(result.stdout).map_err(|_| "scheduler_unavailable")?;
        Ok(value.trim().into())
    }
    #[cfg(not(windows))]
    {
        Err("next_run_unavailable".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escapes_xml_and_quotes_state_paths_without_shell_execution() {
        let mut job = SavedJob {
            id: "aaaaaaaa-0000-4000-8000-000000000001".into(),
            revision: 1,
            name: "test".into(),
            source: "".into(),
            destination: "".into(),
            kind: "local".into(),
            schedule_automatic: true,
            wake_computer: false,
            deletion_guard_enabled: true,
            deletion_guard_min_files: 10,
            retention_days: 30,
            archive_mode: "days".into(),
            schedule_kind: "daily".into(),
            schedule_every: 1,
            daily_minute: 185,
            missed_run: "run_once_when_available".into(),
            enabled: true,
            schedule_enabled: false,
            schedule_pending: false,
            schedule_anchor: "2026-10-19".into(),
            current_path: "".into(),
            destination_layout: "isolated".into(),
            last_result: None,
            last_run: None,
            last_error: None,
        };
        let result = windows_xml(
            &job,
            Path::new("C:/App & Data/cli.exe"),
            Path::new("C:/Data & Test/state.sqlite"),
            "User&Name",
        )
        .unwrap();
        assert!(result.contains("03:05:00"));
        assert!(result.contains("User&amp;Name"));
        assert!(result.contains("&quot;C:/Data &amp; Test/state.sqlite&quot;"));
        assert!(result.contains("<WakeToRun>false"));
        job.wake_computer = true;
        for (kind, every, expected) in [
            ("minutes", 15, "<Interval>PT15M</Interval>"),
            ("hours", 3, "<Interval>PT180M</Interval>"),
            ("days", 7, "<DaysInterval>7</DaysInterval>"),
            ("startup", 1, "<LogonTrigger>"),
        ] {
            job.schedule_kind = kind.into();
            job.schedule_every = every;
            let xml = windows_xml(
                &job,
                Path::new("C:/cli.exe"),
                Path::new("C:/state.sqlite"),
                "test",
            )
            .unwrap();
            assert!(xml.contains(expected));
            assert!(xml.contains("<WakeToRun>true</WakeToRun>"));
            assert!(xml.contains("<Hidden>false</Hidden>"));
            if kind == "startup" {
                assert!(xml.contains("<UserId>test</UserId><Delay>PT30S</Delay>"));
                assert!(!xml.contains("<CalendarTrigger>"));
            }
            if kind == "days" {
                assert!(xml.contains("2026-10-19T03:05:00"));
            }
        }
        job.schedule_automatic = false;
        assert!(windows_xml(
            &job,
            Path::new("C:/cli.exe"),
            Path::new("C:/state.sqlite"),
            "test"
        )
        .is_err());
        job.schedule_automatic = true;
        job.schedule_every = 0;
        assert!(windows_xml(
            &job,
            Path::new("C:/cli.exe"),
            Path::new("C:/state.sqlite"),
            "test"
        )
        .is_err());
    }
}
