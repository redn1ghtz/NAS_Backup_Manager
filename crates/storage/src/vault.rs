//! Per-user encrypted secret blobs. Never put plaintext in SQLite, arguments or logs.
use std::path::Path;

pub fn protect(input: &[u8], decrypt: bool) -> Result<Vec<u8>, &'static str> {
    #[cfg(windows)]
    {
        use base64::{engine::general_purpose::STANDARD, Engine};
        use std::{
            io::Write,
            os::windows::process::CommandExt,
            process::{Command, Stdio},
        };
        let operation = if decrypt { "Unprotect" } else { "Protect" };
        let script = format!("$ErrorActionPreference='Stop'; Add-Type -AssemblyName System.Security; $v=[Convert]::FromBase64String([Console]::In.ReadToEnd()); $r=[Security.Cryptography.ProtectedData]::{operation}($v,$null,[Security.Cryptography.DataProtectionScope]::CurrentUser); [Console]::Out.Write([Convert]::ToBase64String($r))");
        let encoded = STANDARD.encode(
            script
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        let exe = std::env::var_os("SystemRoot").ok_or("vault_unavailable")?;
        let mut child =
            Command::new(Path::new(&exe).join("System32/WindowsPowerShell/v1.0/powershell.exe"))
                .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
                .creation_flags(0x08000000)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|_| "vault_unavailable")?;
        child
            .stdin
            .take()
            .ok_or("vault_unavailable")?
            .write_all(STANDARD.encode(input).as_bytes())
            .map_err(|_| "vault_unavailable")?;
        let output = child.wait_with_output().map_err(|_| "vault_unavailable")?;
        if !output.status.success() {
            return Err("vault_unavailable");
        }
        STANDARD
            .decode(output.stdout)
            .map_err(|_| "vault_unavailable")
    }
    #[cfg(not(windows))]
    {
        let _ = (input, decrypt);
        Err("vault_platform_unsupported")
    }
}

pub fn save_new(path: &Path, plaintext: &[u8]) -> Result<(), &'static str> {
    use std::io::Write;
    let protected = protect(plaintext, false)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "vault_unavailable")?;
    file.write_all(&protected)
        .and_then(|_| file.sync_all())
        .map_err(|_| "vault_unavailable")
}
pub fn load(path: &Path) -> Result<Vec<u8>, &'static str> {
    let m = std::fs::symlink_metadata(path).map_err(|_| "vault_unavailable")?;
    if !m.is_file() || m.len() > 1024 * 1024 {
        return Err("vault_unavailable");
    }
    protect(&std::fs::read(path).map_err(|_| "vault_unavailable")?, true)
}
