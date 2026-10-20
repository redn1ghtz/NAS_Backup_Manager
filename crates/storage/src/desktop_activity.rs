//! Short-lived Windows tray/toast host. Never changes the outcome of a backup.
use std::{
    io::Write,
    path::Path,
    process::Child,
    sync::mpsc,
    time::{Duration, Instant},
};
#[cfg(windows)]
include!(concat!(env!("OUT_DIR"), "/activity_digest.rs"));
pub struct DesktopActivity {
    child: Option<Child>,
    status: mpsc::Receiver<String>,
    last_percent: Option<u32>,
}
impl DesktopActivity {
    pub fn start(
        state: &Path,
        job: &str,
        language: &str,
        notifications: bool,
    ) -> Result<Self, &'static str> {
        #[cfg(windows)]
        {
            use std::{
                io::{BufRead, BufReader},
                os::windows::process::CommandExt,
                process::{Command, Stdio},
            };
            let _ = state;
            let host = installed_host()?;
            let executable = std::env::current_exe().map_err(|_| "notification_unavailable")?;
            let application = ["NAS Backup Manager.exe", "nas-backup-desktop.exe"]
                .iter()
                .map(|name| executable.with_file_name(name))
                .find(|path| path.is_file())
                .unwrap_or(executable);
            let mut child = Command::new(host)
                .creation_flags(0x08000000)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|_| "notification_unavailable")?;
            let output = child.stdout.take().ok_or("notification_unavailable")?;
            let (sender, status) = mpsc::channel();
            std::thread::spawn(move || {
                for line in BufReader::new(output).lines().map_while(Result::ok) {
                    if sender.send(line).is_err() {
                        break;
                    }
                }
            });
            let mut activity = Self {
                child: Some(child),
                status,
                last_percent: None,
            };
            activity.send(serde_json::json!({"application":application,"job":job,"language":language,"notifications":notifications,"owner":std::process::id()}))?;
            Ok(activity)
        }
        #[cfg(not(windows))]
        {
            let _ = (state, job, language, notifications);
            Err("notification_unavailable")
        }
    }
    fn send(&mut self, message: serde_json::Value) -> Result<(), &'static str> {
        let mut bytes = serde_json::to_vec(&message).map_err(|_| "notification_unavailable")?;
        bytes.push(b'\n');
        self.child
            .as_mut()
            .and_then(|child| child.stdin.as_mut())
            .ok_or("notification_unavailable")?
            .write_all(&bytes)
            .map_err(|_| "notification_unavailable")
    }
    pub fn progress(&mut self, completed: i64, total: i64) {
        if total > 0 {
            let percent = ((completed as f64 / total as f64) * 100.).clamp(0., 99.) as u32;
            if self.last_percent != Some(percent)
                && self
                    .send(serde_json::json!({"action":"progress","percent":percent}))
                    .is_ok()
            {
                self.last_percent = Some(percent);
            }
        }
    }
    pub fn set_notifications(&mut self, enabled: bool) {
        let _ = self.send(serde_json::json!({"action":"notifications","enabled":enabled}));
    }
    pub fn finish(mut self, result: &str) -> Result<(), &'static str> {
        self.send(serde_json::json!({"action":"finish","result":result}))?;
        if let Some(child) = self.child.as_mut() {
            child.stdin.take();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut failed = false;
        let mut acknowledged = false;
        loop {
            while let Ok(line) = self.status.try_recv() {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                    failed |= value.get("error").is_some_and(|error| !error.is_null());
                    acknowledged |= value["stage"] == "finished" || value["stage"] == "failed";
                    if result == "failed" && value["stage"] == "finished_error" {
                        self.child.take();
                        return if failed {
                            Err("notification_unavailable")
                        } else {
                            Ok(())
                        };
                    }
                }
            }
            if let Some(child) = self.child.as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    if !status.success() {
                        return Err("notification_unavailable");
                    }
                    if acknowledged {
                        return if failed {
                            Err("notification_unavailable")
                        } else {
                            Ok(())
                        };
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err("notification_unavailable");
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}
impl Drop for DesktopActivity {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(windows)]
fn installed_host() -> Result<std::path::PathBuf, &'static str> {
    let executable = std::env::current_exe().map_err(|_| "notification_unavailable")?;
    let directory = executable.parent().ok_or("notification_unavailable")?;
    checked_host(directory)
}
#[cfg(windows)]
fn checked_host(directory: &Path) -> Result<std::path::PathBuf, &'static str> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let host = directory.join("nas-backup-notify.exe");
    let metadata = std::fs::symlink_metadata(&host).map_err(|_| "notification_unavailable")?;
    if !metadata.file_type().is_file() {
        return Err("notification_unavailable");
    }
    let mut file = std::fs::File::open(&host).map_err(|_| "notification_unavailable")?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| "notification_unavailable")?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    if hash.finalize()[..] != HOST_SHA256 {
        return Err("notification_unavailable");
    }
    Ok(host)
}

#[cfg(all(test, windows))]
mod sidecar_tests {
    use super::*;
    #[test]
    fn only_the_packaged_sidecar_is_accepted_without_runtime_extraction() {
        let directory = tempfile::tempdir().unwrap();
        assert!(checked_host(directory.path()).is_err());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
        let host = directory.path().join("nas-backup-notify.exe");
        std::fs::copy(concat!(env!("OUT_DIR"), "/nas-backup-notify.exe"), &host).unwrap();
        assert_eq!(checked_host(directory.path()).unwrap(), host);
        std::fs::OpenOptions::new()
            .append(true)
            .open(&host)
            .unwrap()
            .write_all(b"tampered")
            .unwrap();
        assert!(checked_host(directory.path()).is_err());
        std::fs::remove_file(&host).unwrap();
        std::fs::create_dir(&host).unwrap();
        assert!(checked_host(directory.path()).is_err());
    }
}
/// Foreground an existing GUI without a resident listener or a new runtime dependency.
pub fn focus_existing(application: &Path) -> Result<bool, &'static str> {
    #[cfg(windows)]
    {
        use std::{
            os::windows::process::CommandExt,
            process::{Command, Stdio},
        };
        let host = installed_host()?;
        let status = Command::new(host)
            .arg("--focus")
            .arg(application)
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|_| "activation_failed")?;
        match status.code() {
            Some(0) => Ok(true),
            Some(2) => Ok(false),
            _ => Err("activation_failed"),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = application;
        Ok(false)
    }
}
