//! OS SMB connection bootstrap and per-user credential vault. No custom SMB stack.
#[cfg(not(windows))]
use std::path::Path;

pub fn share_root(path: &str) -> Result<String, &'static str> {
    let path = if path
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(r"\\?\UNC\"))
    {
        format!(r"\\{}", &path[8..])
    } else {
        path.to_owned()
    };
    let tail = path.strip_prefix(r"\\").ok_or("network_unc_required")?;
    let parts: Vec<_> = tail.split('\u{005c}').collect();
    if parts.len() < 2
        || parts
            .iter()
            .any(|part| part.contains(['\n', '\r', '\0', '/']) || *part == ".." || *part == ".")
        || parts[0].is_empty()
        || parts[1].is_empty()
        || parts[0] == "?"
        || parts[0] == "."
    {
        return Err("network_unc_required");
    }
    Ok(format!(
        r"\\{}\{}",
        parts[0].to_lowercase(),
        parts[1].to_lowercase()
    ))
}

#[cfg(windows)]
fn invoke(action: &str, path: &str, username: &str, password: &str) -> Result<u32, &'static str> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use std::{
        io::Write,
        os::windows::process::CommandExt,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let share = share_root(path)?;
    let bytes: Vec<_> = include_str!("windows_network.ps1")
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let executable = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .ok_or("network_helper_unavailable")?
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut child = Command::new(executable)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-EncodedCommand",
            &STANDARD.encode(bytes),
        ])
        .creation_flags(0x08000000)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "network_helper_unavailable")?;
    // Secrets go only through a private pipe; never argv, files, SQLite or logs.
    let request = serde_json::to_vec(
        &serde_json::json!({"action":action,"share":share,"username":username,"password":password,"path":path}),
    )
    .map_err(|_| "network_helper_unavailable")?;
    let sent = child
        .stdin
        .take()
        .ok_or("network_helper_unavailable")
        .and_then(|mut pipe| {
            pipe.write_all(&request)
                .map_err(|_| "network_helper_unavailable")
        });
    if sent.is_err() {
        let _ = child.kill();
        let _ = child.wait();
        return Err("network_helper_unavailable");
    }
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < Duration::from_secs(30) => {
                std::thread::sleep(Duration::from_millis(50))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("network_connection_timeout");
            }
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|_| "network_helper_unavailable")?;
    std::str::from_utf8(&output.stdout)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .ok_or("network_helper_unavailable")
}
#[cfg(not(windows))]
fn invoke(_: &str, _: &str, _: &str, _: &str) -> Result<u32, &'static str> {
    Err("network_credentials_unsupported")
}

fn result(code: u32) -> Result<(), &'static str> {
    match code {
        0 => Ok(()),
        1272 => Err("network_guest_blocked"),
        100003 => Err("network_guest_rejected"),
        1219 => Err("network_credential_conflict"),
        5 | 86 | 1326 | 1909 => Err("network_auth_failed"),
        53 | 64 | 67 | 1231 | 1232 => Err("network_unavailable"),
        1168 => Err("network_credentials_missing"),
        _ => Err("network_helper_unavailable"),
    }
}
pub fn credentials_saved(path: &str) -> Result<bool, &'static str> {
    match invoke("status", path, "", "")? {
        0 => Ok(true),
        1168 => Ok(false),
        code => {
            result(code)?;
            Ok(false)
        }
    }
}
/// Mode metadata only; never returns usernames or secret blobs to the UI.
pub fn access_mode(path: &str) -> Result<&'static str, &'static str> {
    match invoke("profile", path, "", "")? {
        100000 => Ok("windows"),
        100001 => Ok("guest"),
        100002 => Ok("credentials"),
        code => {
            result(code)?;
            Err("network_helper_unavailable")
        }
    }
}
/// A guest preference needs no password or network access to be saved.
pub fn save_guest_access(path: &str) -> Result<(), &'static str> {
    result(invoke("store_guest", path, "guest", "")?)
}
pub fn save_credentials(path: &str, username: &str, password: &str) -> Result<(), &'static str> {
    if username.trim().is_empty()
        || password.is_empty()
        || password.encode_utf16().count() > 1280
        || username.contains(['\n', '\r', '\0'])
        || password.contains('\0')
    {
        return Err("network_credentials_invalid");
    }
    result(invoke("save", path, username, password)?)
}
pub fn forget_credentials(path: &str) -> Result<(), &'static str> {
    result(invoke("forget", path, "", "")?)
}
/// Called once per explicit operation, never per file or from an idle polling loop.
pub fn ensure_connected(path: &str) -> Result<(), &'static str> {
    #[cfg(windows)]
    {
        share_root(path)?;
        result(invoke("connect", path, "", "")?)
    }
    #[cfg(not(windows))]
    {
        let _ = Path::new(path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guest_errors_are_actionable_without_changing_os_policy() {
        assert_eq!(result(1272).unwrap_err(), "network_guest_blocked");
        assert_eq!(result(100003).unwrap_err(), "network_guest_rejected");
        assert_eq!(result(1219).unwrap_err(), "network_credential_conflict");
    }
    #[test]
    fn share_parser_rejects_devices_and_traversal() {
        assert_eq!(
            share_root(r"\\?\unc\SERVER\SHARE\folder").unwrap(),
            r"\\server\share"
        );
        assert_eq!(
            share_root(r"\\SERVER\Share\folder").unwrap(),
            r"\\server\share"
        );
        assert_eq!(
            share_root(r"\\?\UNC\Server\Share\folder").unwrap(),
            r"\\server\share"
        );
        for path in [
            r"C:\local",
            r"\\.\pipe\test",
            r"\\?\C:\local",
            r"\\server",
            r"\\server\share\..\other",
            "\\\\server\\share\nsecret",
        ] {
            assert!(share_root(path).is_err());
        }
    }
    #[test]
    #[cfg(windows)]
    #[ignore = "opt-in: writes/removes only a UUID-named dummy Windows credential; no network access"]
    fn disposable_windows_vault_round_trip() {
        let share = format!(
            r"\\nas-backup-test-{}\.test",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        struct Cleanup(String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = forget_credentials(&self.0);
            }
        }
        let _cleanup = Cleanup(share.clone());
        assert_eq!(
            invoke(
                "store",
                &share,
                "disposable-user",
                "test-only-unused-secret"
            )
            .unwrap(),
            0
        );
        assert!(credentials_saved(&share).unwrap());
        assert_eq!(access_mode(&share).unwrap(), "credentials");
        save_guest_access(&share).unwrap();
        assert!(credentials_saved(&share).unwrap());
        assert_eq!(access_mode(&share).unwrap(), "guest");
        forget_credentials(&share).unwrap();
        assert!(!credentials_saved(&share).unwrap());
        assert_eq!(access_mode(&share).unwrap(), "windows");
    }
}
