//! Application-owned encrypted rclone connections. OAuth remains in the user's browser.
use crate::{
    advanced,
    backup::{now, Result},
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};

#[derive(Serialize, Deserialize)]
struct Metadata {
    provider: String,
    executable: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudConnection {
    pub connection_id: String,
    pub provider: String,
    pub config: String,
    pub executable: String,
    pub connected: bool,
}
fn directory(db: &Connection, id: &str) -> Result<PathBuf> {
    crate::model::JobId::parse(id).map_err(|_| advanced::error("invalid_cloud_connection"))?;
    let root = advanced::state_directory(db)?.join("cloud");
    for p in [&root, &root.join(id)] {
        if fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(advanced::error("invalid_cloud_connection"));
        }
    }
    Ok(root.join(id))
}
pub fn password(db: &Connection, id: &str) -> Result<String> {
    String::from_utf8(
        nas_backup_storage::vault::load(&directory(db, id)?.join("password.dpapi"))
            .map_err(advanced::error)?,
    )
    .map_err(|_| advanced::error("vault_unavailable"))
}
pub fn status(db: &Connection, id: &str) -> Result<CloudConnection> {
    let dir = directory(db, id)?;
    let meta: Metadata = serde_json::from_slice(&fs::read(dir.join("connection.json"))?)
        .map_err(|_| advanced::error("invalid_cloud_connection"))?;
    if !["onedrive", "dropbox"].contains(&meta.provider.as_str()) {
        return Err(advanced::error("cloud_provider_unavailable"));
    }
    Ok(CloudConnection {
        connection_id: id.into(),
        provider: meta.provider,
        executable: meta.executable,
        config: dir.join("rclone.conf").to_string_lossy().into_owned(),
        connected: dir.join("connected").is_file(),
    })
}
fn wait(
    child: &mut std::process::Child,
    timeout: Duration,
    cancelled: impl Fn() -> bool,
) -> Result<()> {
    let started = Instant::now();
    loop {
        if cancelled() || started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(advanced::error(if started.elapsed() > timeout {
                "cloud_auth_timeout"
            } else {
                "cancelled"
            }));
        }
        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(advanced::error("cloud_auth_failed"))
            };
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
pub fn prepare(db: &Connection, provider: &str, executable: &str) -> Result<CloudConnection> {
    if !["onedrive", "dropbox"].contains(&provider) {
        return Err(advanced::error("invalid_cloud_connection"));
    }
    advanced::executable_path(executable)?;
    let id = uuid::Uuid::new_v4().to_string();
    let dir = directory(db, &id)?;
    fs::create_dir_all(&dir)?;
    // 244 random bits. The password is never sent to the UI or command arguments.
    let pass = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    nas_backup_storage::vault::save_new(&dir.join("password.dpapi"), pass.as_bytes())
        .map_err(advanced::error)?;
    fs::write(
        dir.join("connection.json"),
        serde_json::to_vec(&Metadata {
            provider: provider.into(),
            executable: executable.into(),
        })
        .map_err(|_| advanced::error("invalid_cloud_connection"))?,
    )?;
    // Encrypt an empty config BEFORE any OAuth credential can be stored.
    let mut child = advanced::command(executable)?
        .arg("--config")
        .arg(dir.join("rclone.conf"))
        .args(["config", "encryption", "set"])
        .env_remove("RCLONE_PASSWORD_COMMAND")
        .env_remove("RCLONE_CONFIG_PASS")
        .stdin(Stdio::piped())
        .spawn()?;
    let input = child
        .stdin
        .take()
        .ok_or_else(|| advanced::error("cloud_auth_failed"))?;
    let mut input = input;
    if let Err(e) = input.write_all(format!("{pass}\n{pass}\n").as_bytes()) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(e.into());
    }
    drop(input);
    wait(&mut child, Duration::from_secs(30), || false)?;
    require_encrypted(&dir.join("rclone.conf"))?;
    status(db, &id)
}
fn require_encrypted(path: &std::path::Path) -> Result<()> {
    if fs::metadata(path)?.len() > 1024 * 1024
        || !fs::read_to_string(path)?
            .lines()
            .any(|s| s == "RCLONE_ENCRYPT_V0:")
    {
        return Err(advanced::error("invalid_cloud_connection"));
    }
    Ok(())
}
fn configure(
    db: &Connection,
    id: &str,
    kind: &str,
    parameters: &[&str],
) -> Result<CloudConnection> {
    let dir = directory(db, id)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("authorization.lock"))?;
    fs2::FileExt::try_lock_exclusive(&lock).map_err(|_| advanced::error("job_busy"))?;
    let meta: Metadata = serde_json::from_slice(&fs::read(dir.join("connection.json"))?)
        .map_err(|_| advanced::error("invalid_cloud_connection"))?;
    require_encrypted(&dir.join("rclone.conf"))?;
    if dir.join("cancel").exists() {
        return Err(advanced::error("cancelled"));
    }
    let mut command = advanced::command(&meta.executable)?;
    command
        .env_remove("RCLONE_PASSWORD_COMMAND")
        .env_remove("RCLONE_CONFIG_BACKUP_CLIENT_ID")
        .env_remove("RCLONE_CONFIG_BACKUP_CLIENT_SECRET");
    command
        .arg("--config")
        .arg(dir.join("rclone.conf"))
        .args([
            "--log-level",
            "ERROR",
            "--contimeout",
            "15s",
            "--timeout",
            "60s",
            "config",
            "create",
            "backup",
            kind,
        ])
        .args(parameters)
        .arg("--no-output")
        .env("RCLONE_CONFIG_PASS", password(db, id)?);
    let mut child = command.spawn()?;
    wait(&mut child, Duration::from_secs(300), || {
        dir.join("cancel").exists()
    })?;
    require_encrypted(&dir.join("rclone.conf"))?;
    crate::remote::check_connection_access(
        &meta.executable,
        &dir.join("rclone.conf"),
        &password(db, id)?,
        &mut || {
            if dir.join("cancel").exists() {
                Err(advanced::error("cancelled"))
            } else {
                Ok(())
            }
        },
    )?;
    fs::write(dir.join("connected"), now().to_string())?;
    status(db, id)
}
pub fn authorize(db: &Connection, id: &str) -> Result<CloudConnection> {
    let s = status(db, id)?;
    match s.provider.as_str() {
        "onedrive" => configure(
            db,
            id,
            "onedrive",
            &[
                "config_is_local",
                "true",
                "config_type",
                "onedrive",
                "config_drive_ok",
                "true",
                "disable_site_permission",
                "true",
            ],
        ),
        "dropbox" => configure(db, id, "dropbox", &["config_is_local", "true"]),
        _ => Err(advanced::error("invalid_cloud_connection")),
    }
}
pub fn cancel(db: &Connection, id: &str) -> Result<()> {
    fs::write(directory(db, id)?.join("cancel"), b"cancel")?;
    Ok(())
}
pub fn validate(db: &Connection, o: &advanced::Options) -> Result<()> {
    if o.cloud_connection.is_empty() {
        return Ok(());
    }
    let s = status(db, &o.cloud_connection)?;
    if !s.connected
        || s.config != o.rclone_config
        || s.provider != o.cloud_provider
        || !o.cloud_remote.starts_with("backup:")
    {
        return Err(advanced::error("invalid_cloud_connection"));
    }
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn retired_connection_remains_on_disk_but_cannot_be_used() {
        let t = tempfile::tempdir().unwrap();
        let db = nas_backup_storage::database::Database::open(&t.path().join("state.db")).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let dir = super::directory(&db.connection, &id).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("connection.json"),
            br#"{"provider":"retired-provider","executable":"unused"}"#,
        )
        .unwrap();
        std::fs::write(dir.join("connected"), b"preserve").unwrap();
        assert_eq!(
            super::status(&db.connection, &id).err().unwrap().code,
            "cloud_provider_unavailable"
        );
        assert!(dir.join("connection.json").is_file());
        assert_eq!(std::fs::read(dir.join("connected")).unwrap(), b"preserve");
        assert_eq!(
            super::prepare(&db.connection, "retired-provider", "unused")
                .err()
                .unwrap()
                .code,
            "invalid_cloud_connection"
        );
    }

    #[test]
    #[ignore = "requires rclone; only offline configuration questions, no account or browser"]
    fn supported_providers_reach_browser_signin_without_user_client_keys() {
        let t = tempfile::tempdir().unwrap();
        let db = nas_backup_storage::database::Database::open(&t.path().join("state.db")).unwrap();
        let tool = std::env::var("NBM_TEST_RCLONE").unwrap();
        for provider in ["onedrive", "dropbox"] {
            let c = super::prepare(&db.connection, provider, &tool).unwrap();
            let out = crate::advanced::command(&tool)
                .unwrap()
                .arg("--config")
                .arg(&c.config)
                .args(["config", "create", "backup", provider, "--non-interactive"])
                .env(
                    "RCLONE_CONFIG_PASS",
                    super::password(&db.connection, &c.connection_id).unwrap(),
                )
                .stdout(std::process::Stdio::piped())
                .output()
                .unwrap();
            assert!(out.status.success());
            let question: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(question["Option"]["Name"], "config_is_local", "{provider}");
            super::require_encrypted(std::path::Path::new(&c.config)).unwrap();
        }
    }
    #[test]
    #[ignore = "requires explicit rclone; no cloud account is used"]
    fn managed_configuration_is_encrypted_before_auth_and_never_returns_password() {
        let t = tempfile::tempdir().unwrap();
        let db = nas_backup_storage::database::Database::open(&t.path().join("state.db")).unwrap();
        let tool = std::env::var("NBM_TEST_RCLONE").unwrap();
        let c = super::prepare(&db.connection, "onedrive", &tool).unwrap();
        assert!(!c.connected);
        let pass = super::password(&db.connection, &c.connection_id).unwrap();
        assert!(!serde_json::to_string(&c).unwrap().contains(&pass));
        let remote = t.path().join("remote-fixture");
        std::fs::create_dir(&remote).unwrap();
        let result = super::configure(
            &db.connection,
            &c.connection_id,
            "alias",
            &["remote", remote.to_str().unwrap()],
        )
        .unwrap();
        assert!(result.connected);
        super::require_encrypted(std::path::Path::new(&result.config)).unwrap();
        let options = crate::advanced::Options {
            cloud_connection: c.connection_id.clone(),
            cloud_provider: "onedrive".into(),
            cloud_mode: "replica".into(),
            cloud_remote: "backup:Folder".into(),
            rclone_config: result.config.clone(),
            ..Default::default()
        };
        super::validate(&db.connection, &options).unwrap();
        assert_eq!(
            crate::advanced::secrets(&db.connection, &options)
                .unwrap()
                .rclone_password,
            pass
        );
        let data = std::fs::read_to_string(result.config).unwrap();
        assert!(!data.contains(&pass));
        assert!(!data.contains("[backup]"));
        super::cancel(&db.connection, &c.connection_id).unwrap();
        assert_eq!(
            super::configure(
                &db.connection,
                &c.connection_id,
                "alias",
                &["remote", remote.to_str().unwrap()]
            )
            .err()
            .unwrap()
            .code,
            "cancelled"
        );
    }
}
