//! Process adapters: arguments are passed directly, never through a shell.
use crate::{
    advanced::{self, Options, Secrets},
    backup::{self, Result, SavedJob},
};
use rusqlite::{params, Connection};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Generate a standard age identity and save an independent recovery file first.
/// Never overwrite an existing recovery key, including a symlink.
pub fn generate_identity(executable: &str, destination: &Path) -> Result<(String, String)> {
    if !destination.is_absolute() {
        return Err(advanced::error("invalid_encryption_key"));
    }
    let output = advanced::command(executable)?
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()?;
    if !output.status.success() || output.stdout.len() > 65536 {
        return Err(advanced::error("invalid_encryption_key"));
    }
    let identity =
        String::from_utf8(output.stdout).map_err(|_| advanced::error("invalid_encryption_key"))?;
    let recipient = identity
        .lines()
        .find_map(|line| line.strip_prefix("# public key: "))
        .filter(|key| key.starts_with("age1"))
        .ok_or_else(|| advanced::error("invalid_encryption_key"))?
        .to_owned();
    if !identity
        .lines()
        .any(|line| line.starts_with("AGE-SECRET-KEY-"))
    {
        return Err(advanced::error("invalid_encryption_key"));
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(destination).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            advanced::error("recovery_key_exists")
        } else {
            e.into()
        }
    })?;
    if let Err(error) = file
        .write_all(identity.as_bytes())
        .and_then(|_| file.sync_all())
    {
        drop(file);
        let _ = fs::remove_file(destination);
        return Err(error.into());
    }
    Ok((recipient, identity))
}

fn rclone(o: &Options, s: &Secrets) -> Result<Command> {
    if !o.cloud_provider.is_empty() && !["onedrive", "dropbox"].contains(&o.cloud_provider.as_str())
    {
        return Err(advanced::error("cloud_provider_unavailable"));
    }
    let mut c = advanced::command(&o.rclone_executable)?;
    c.args([
        "--config",
        &o.rclone_config,
        "--ask-password=false",
        "--log-level",
        "ERROR",
        "--contimeout",
        "15s",
        "--timeout",
        "60s",
        "--retries",
        "1",
        "--low-level-retries",
        "1",
        "--checkers",
        "1",
        "--transfers",
        "1",
        "--buffer-size",
        "1M",
        "--max-buffer-memory",
        "32M",
        "--max-backlog",
        "128",
        "--multi-thread-streams",
        "0",
    ]);
    if o.bandwidth_kib > 0 {
        let a = o.bandwidth_start_minute;
        let b = o.bandwidth_end_minute;
        let value = if a == b {
            format!("{}K", o.bandwidth_kib)
        } else {
            format!(
                "{:02}:{:02},{}K {:02}:{:02},off",
                a / 60,
                a % 60,
                o.bandwidth_kib,
                b / 60,
                b % 60
            )
        };
        c.args(["--bwlimit", &value]);
    }
    c.env("RCLONE_CONFIG_PASS", &s.rclone_password);
    c.env_remove("RCLONE_CONFIG_BACKUP_CLIENT_ID")
        .env_remove("RCLONE_CONFIG_BACKUP_CLIENT_SECRET");
    // Bound provider upload buffers as well as rclone's streaming buffer.
    c.args(["--dropbox-chunk-size", "8M", "--onedrive-chunk-size", "5M"]);
    Ok(c)
}
#[derive(Clone, Debug)]
pub struct CloudTransfer {
    pub bytes: i64,
    pub total_bytes: i64,
    pub completed: i64,
    pub total: i64,
    pub eta_seconds: Option<i64>,
    pub current_path: Option<String>,
}
fn cloud_stats(line: &[u8]) -> Option<CloudTransfer> {
    let value: serde_json::Value = serde_json::from_slice(line).ok()?;
    let stats = value.get("stats")?;
    let bytes = stats.get("bytes")?.as_i64()?.max(0);
    let total_bytes = stats.get("totalBytes")?.as_i64()?.max(bytes);
    let elapsed = stats
        .get("elapsedTime")
        .and_then(|n| n.as_f64())
        .unwrap_or(0.0);
    let speed = stats
        .get("speed")
        .and_then(|n| n.as_f64())
        .filter(|n| n.is_finite() && *n > 0.0)
        .or_else(|| (elapsed >= 2.0 && bytes > 0).then_some(bytes as f64 / elapsed));
    let remaining = total_bytes.saturating_sub(bytes);
    let eta_seconds = (remaining > 0)
        .then(|| {
            stats
                .get("eta")
                .and_then(|n| n.as_f64())
                .filter(|n| n.is_finite() && *n > 0.0)
                .or_else(|| speed.map(|rate| remaining as f64 / rate))
                .map(|n| n.ceil().max(1.0) as i64)
        })
        .flatten();
    Some(CloudTransfer {
        bytes,
        total_bytes,
        completed: stats
            .get("transfers")
            .and_then(|n| n.as_i64())
            .unwrap_or(0)
            .max(0),
        total: stats
            .get("totalTransfers")
            .and_then(|n| n.as_i64())
            .unwrap_or(0)
            .max(0),
        eta_seconds,
        current_path: stats
            .get("transferring")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.chars().take(512).collect()),
    })
}
// At most one bounded JSON line and the latest numeric sample are retained.
fn progress_line(
    buffer: &mut Vec<u8>,
    skip: &mut bool,
    bytes: &[u8],
    latest: &Mutex<Option<CloudTransfer>>,
) {
    for &byte in bytes {
        if byte == b'\n' {
            if !*skip {
                if let Some(sample) = cloud_stats(buffer) {
                    if let Ok(mut slot) = latest.lock() {
                        *slot = Some(sample);
                    }
                }
            }
            buffer.clear();
            *skip = false;
        } else if !*skip {
            if buffer.len() < 65536 {
                buffer.push(byte);
            } else {
                buffer.clear();
                *skip = true;
            }
        }
    }
}
fn run(cmd: Command, limit: Duration, check: &mut impl FnMut() -> Result<()>) -> Result<i32> {
    run_progress(cmd, limit, check, &mut |_| Ok(()))
}
fn run_progress(
    mut cmd: Command,
    limit: Duration,
    check: &mut impl FnMut() -> Result<()>,
    notify: &mut impl FnMut(CloudTransfer) -> Result<()>,
) -> Result<i32> {
    let mut child = cmd.stderr(Stdio::piped()).spawn()?;
    let mut errors = child
        .stderr
        .take()
        .ok_or_else(|| advanced::error("external_tool_failed"))?;
    // Only fixed diagnostic codes leave this bounded reader. Raw tool output can
    // contain credentials and must never enter the database, UI or a log file.
    let latest = Arc::new(Mutex::new(None));
    let reader_latest = latest.clone();
    let reader = std::thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        let mut tail = Vec::new();
        let mut code = None;
        let mut line = Vec::new();
        let mut skip = false;
        while let Ok(n) = errors.read(&mut buffer) {
            if n == 0 {
                break;
            }
            progress_line(&mut line, &mut skip, &buffer[..n], &reader_latest);
            tail.extend_from_slice(&buffer[..n]);
            code = classify_remote_error(&tail).or(code);
            if tail.len() > 128 {
                tail.drain(..tail.len() - 128);
            }
        }
        progress_line(&mut line, &mut skip, b"\n", &reader_latest);
        code
    });
    let start = Instant::now();
    let mut last_progress = Instant::now();
    let outcome = (|| loop {
        check()?;
        if last_progress.elapsed() >= Duration::from_millis(500) {
            let sample = latest.lock().ok().and_then(|mut s| s.take());
            if let Some(sample) = sample {
                notify(sample)?;
            }
            last_progress = Instant::now();
        }
        if start.elapsed() > limit {
            return Err(advanced::error("external_timeout"));
        }
        if let Some(status) = child.try_wait()? {
            return Ok(status.code().unwrap_or(-1));
        }
        std::thread::sleep(Duration::from_millis(100));
    })();
    if outcome.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let diagnostic = reader.join().unwrap_or(None);
    if outcome.is_ok() {
        let sample = latest.lock().ok().and_then(|mut s| s.take());
        if let Some(sample) = sample {
            notify(sample)?;
        }
    }
    match outcome {
        Ok(code) if code != 0 && diagnostic.is_some() => Err(advanced::error(diagnostic.unwrap())),
        result => result,
    }
}
fn classify_remote_error(bytes: &[u8]) -> Option<&'static str> {
    let text = String::from_utf8_lossy(bytes).to_ascii_lowercase();
    if ["[410 - goneerror]", "410 gone", "http status 410"]
        .iter()
        .any(|s| text.contains(s))
    {
        Some("cloud_api_unavailable")
    } else if [
        "[401 -",
        "401 unauthorized",
        "invalid_grant",
        "invalid token",
    ]
    .iter()
    .any(|s| text.contains(s))
    {
        Some("cloud_authentication_failed")
    } else if ["[403 -", "403 forbidden"].iter().any(|s| text.contains(s)) {
        Some("cloud_permission_denied")
    } else {
        None
    }
}
pub fn check_connection_access(
    executable: &str,
    config: &Path,
    password: &str,
    check: &mut impl FnMut() -> Result<()>,
) -> Result<()> {
    let options = Options {
        rclone_executable: executable.into(),
        rclone_config: config.to_string_lossy().into_owned(),
        ..Default::default()
    };
    let secrets = Secrets {
        rclone_password: password.into(),
        ..Default::default()
    };
    let mut cmd = rclone(&options, &secrets)?;
    cmd.args(["lsf", "backup:", "--max-depth", "1", "--dirs-only"]);
    if run(cmd, Duration::from_secs(90), check)? == 0 {
        Ok(())
    } else {
        Err(advanced::error("cloud_failed"))
    }
}
fn retry(
    cmd: impl Fn() -> Result<Command>,
    o: &Options,
    check: &mut impl FnMut() -> Result<()>,
) -> Result<()> {
    for attempt in 0..=o.retry_attempts {
        let code = run(cmd()?, Duration::from_secs(7 * 86400), check)?;
        if code == 0 {
            return Ok(());
        }
        if code != 5 || attempt == o.retry_attempts {
            return Err(advanced::error("cloud_failed"));
        }
        advanced::wait(advanced::retry_delay(o, attempt), check)?;
    }
    Err(advanced::error("cloud_failed"))
}
fn transfer(
    cmd: impl Fn() -> Result<Command>,
    o: &Options,
    check: &mut impl FnMut() -> Result<()>,
    notify: &mut impl FnMut(CloudTransfer) -> Result<()>,
) -> Result<()> {
    for attempt in 0..=o.retry_attempts {
        let mut command = cmd()?;
        command.args([
            "--use-json-log",
            "--stats",
            "500ms",
            "--stats-log-level",
            "NOTICE",
            "--log-level",
            "NOTICE",
        ]);
        let code = run_progress(command, Duration::from_secs(7 * 86400), check, notify)?;
        if code == 0 {
            return Ok(());
        }
        if code != 5 || attempt == o.retry_attempts {
            return Err(advanced::error("cloud_failed"));
        }
        advanced::wait(advanced::retry_delay(o, attempt), check)?;
    }
    Err(advanced::error("cloud_failed"))
}
fn fetch_small(
    o: &Options,
    s: &Secrets,
    args: &[&str],
    folder: &Path,
    check: &mut impl FnMut() -> Result<()>,
) -> Result<(i32, Vec<u8>)> {
    let path = folder.join(format!("{}.remote", uuid::Uuid::new_v4()));
    let output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    let mut cmd = rclone(o, s)?;
    cmd.args(args).stdout(Stdio::from(output));
    let result = run(cmd, Duration::from_secs(90), &mut || {
        check()?;
        if fs::metadata(&path)?.len() > 65536 {
            return Err(advanced::error("remote_not_empty"));
        }
        Ok(())
    })
    .and_then(|code| {
        if fs::metadata(&path)?.len() > 65536 {
            return Err(advanced::error("remote_not_empty"));
        }
        Ok((code, fs::read(&path)?))
    });
    let _ = fs::remove_file(path);
    result
}
pub fn replicate(
    db: &Connection,
    job: &SavedJob,
    check: &mut impl FnMut() -> Result<()>,
    notify: &mut impl FnMut(&str, CloudTransfer) -> Result<()>,
) -> Result<()> {
    let o = &job.advanced;
    if o.cloud_mode == "off" {
        return Ok(());
    }
    let s = advanced::secrets(db, o)?;
    advanced::remote_parts(&o.cloud_remote)?;
    let folder = advanced::state_directory(db)?.join("activity");
    fs::create_dir_all(&folder)?;
    let owner = fs::read(backup::root(job).join("owner"))?;
    let marker = format!("{}/.nas-backup-manager/cloud-owner", o.cloud_remote);
    let (status, remote_owner) = fetch_small(o, &s, &["cat", &marker], &folder, check)?;
    if status == 0 {
        if remote_owner != owner {
            return Err(advanced::error("ownership_mismatch"));
        }
    } else {
        // Never initialize ownership over a non-empty or inaccessible remote folder.
        let (code, listing) = fetch_small(
            o,
            &s,
            &["lsf", &o.cloud_remote, "--max-depth", "1"],
            &folder,
            check,
        )?;
        if code != 0 && code != 3 {
            return Err(advanced::error("cloud_failed"));
        }
        if !listing.is_empty() {
            return Err(advanced::error("remote_not_empty"));
        }
        let file = folder.join(format!("{}.owner", uuid::Uuid::new_v4()));
        fs::write(&file, &owner)?;
        let result = retry(
            || {
                let mut c = rclone(o, &s)?;
                c.arg("copyto").arg(&file).arg(&marker);
                Ok(c)
            },
            o,
            check,
        );
        let _ = fs::remove_file(file);
        result?;
        let (code, value) = fetch_small(o, &s, &["cat", &marker], &folder, check)?;
        if code != 0 || value != owner {
            return Err(advanced::error("ownership_mismatch"));
        }
    }
    // Alias/root-folder configurations can hide an overlap from string comparison.
    // Refuse nested job markers before any sync can replace another job's files.
    let (code, nested) = fetch_small(
        o,
        &s,
        &[
            "lsf",
            &o.cloud_remote,
            "--recursive",
            "--files-only",
            "--include",
            "**/.nas-backup-manager/cloud-owner",
        ],
        &folder,
        check,
    )?;
    if code != 0 {
        return Err(advanced::error("cloud_failed"));
    }
    if String::from_utf8_lossy(&nested)
        .lines()
        .any(|line| line.trim() != ".nas-backup-manager/cloud-owner")
    {
        return Err(advanced::error("overlap"));
    }
    backup::verify_resources(db, job, true)?;
    db.execute(
        "DELETE FROM cloud_integrity WHERE job_id=?1 AND remote=?2",
        params![job.id, o.cloud_remote],
    )?;
    let versions = format!(
        "{}/.nas-backup-manager/cloud-versions/{}",
        o.cloud_remote,
        uuid::Uuid::new_v4()
    );
    transfer(
        || {
            let mut c = rclone(o, &s)?;
            c.arg("sync")
                .arg(backup::current(job))
                .arg(&o.cloud_remote)
                .args([
                    "--exclude",
                    "/.nas-backup-manager/**",
                    "--delete-after",
                    "--create-empty-src-dirs",
                ]);
            if job.archive_mode == "days" {
                c.args(["--backup-dir", &versions]);
            }
            if job.deletion_guard_enabled {
                c.args([
                    "--max-delete",
                    &job.deletion_guard_min_files.saturating_sub(1).to_string(),
                ]);
            }
            if o.bandwidth_kib > 0 {
                let a = o.bandwidth_start_minute;
                let b = o.bandwidth_end_minute;
                let value = if a == b {
                    format!("{}K", o.bandwidth_kib)
                } else {
                    format!(
                        "{:02}:{:02},{}K {:02}:{:02},off",
                        a / 60,
                        a % 60,
                        o.bandwidth_kib,
                        b / 60,
                        b % 60
                    )
                };
                c.args(["--bwlimit", &value]);
            }
            Ok(c)
        },
        o,
        check,
        &mut |p| notify("cloud", p),
    )?;
    // Replicate immutable archive payloads additively. No remote retention purge.
    notify(
        "cloudArchive",
        CloudTransfer {
            bytes: 0,
            total_bytes: 0,
            completed: 0,
            total: 0,
            eta_seconds: None,
            current_path: None,
        },
    )?;
    transfer(
        || {
            let mut c = rclone(o, &s)?;
            c.arg("copy")
                .arg(backup::root(job).join("archive"))
                .arg(format!("{}/.nas-backup-manager/archive", o.cloud_remote))
                .arg("--immutable");
            Ok(c)
        },
        o,
        check,
        &mut |p| notify("cloudArchive", p),
    )?;
    let tx = db.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM cloud_integrity WHERE job_id=?1 AND remote=?2",
        params![job.id, o.cloud_remote],
    )?;
    tx.execute("INSERT INTO cloud_integrity SELECT job_id,?2,path,sha256 FROM file_integrity WHERE job_id=?1",params![job.id,o.cloud_remote])?;
    tx.execute("INSERT INTO cloud_integrity SELECT job_id,?2,'.nas-backup-manager/archive/'||stored_relative_path,content_hash FROM archive_versions WHERE job_id=?1 AND state='verified'",params![job.id,o.cloud_remote])?;
    tx.commit()?;
    Ok(())
}

pub fn verify_cloud(
    db: &Connection,
    job: &SavedJob,
    check: &mut impl FnMut() -> Result<()>,
) -> Result<()> {
    let o = &job.advanced;
    if o.cloud_mode == "off" {
        return Ok(());
    }
    let count: i64 = db.query_row(
        "SELECT count(*) FROM cloud_integrity WHERE job_id=?1 AND remote=?2",
        params![job.id, o.cloud_remote],
        |r| r.get(0),
    )?;
    if count == 0 {
        return Ok(());
    }
    let s = advanced::secrets(db, o)?;
    let folder = advanced::state_directory(db)?.join("activity");
    fs::create_dir_all(&folder)?;
    let file = folder.join(format!("{}.sha256", uuid::Uuid::new_v4()));
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&file)?;
    let mut q = db.prepare(
        "SELECT path,sha256 FROM cloud_integrity WHERE job_id=?1 AND remote=?2 ORDER BY path",
    )?;
    for row in q.query_map(params![job.id, o.cloud_remote], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        check()?;
        let (path, hash) = row?;
        writeln!(output, "{hash}  {path}")?;
    }
    drop(output);
    let result = (|| -> Result<()> {
        let mut cmd = rclone(o, &s)?;
        cmd.args(["checksum", "SHA-256"])
            .arg(&file)
            .arg(&o.cloud_remote)
            .args([
                "--download",
                "--one-way",
                "--bwlimit",
                &if o.verify_mib_per_second == 0 {
                    "off".to_owned()
                } else {
                    format!("{}M", o.verify_mib_per_second)
                },
            ]);
        let code = run(cmd, Duration::from_secs(7 * 86400), check)?;
        if code == 0 {
            Ok(())
        } else {
            Err(advanced::error("integrity_failed"))
        }
    })();
    let _ = fs::remove_file(&file);
    result
}

fn quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    )
}
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Quota {
    pub total: Option<i64>,
    pub used: Option<i64>,
    pub free: Option<i64>,
}
pub fn capacity(db: &Connection, job: &SavedJob, force: bool) -> Result<Quota> {
    use rusqlite::OptionalExtension;
    if job.advanced.cloud_mode == "off" {
        return Err(advanced::error("cloud_disabled"));
    }
    if !force {
        if let Some(value) = db
            .query_row(
                "SELECT total,used,free FROM cloud_capacity WHERE job_id=?1 AND queried_at>?2",
                params![job.id, backup::now() - 300],
                |r| {
                    Ok(Quota {
                        total: r.get(0)?,
                        used: r.get(1)?,
                        free: r.get(2)?,
                    })
                },
            )
            .optional()?
        {
            return Ok(value);
        }
    }
    let o = &job.advanced;
    let s = advanced::secrets(db, o)?;
    let (name, _) = advanced::remote_parts(&o.cloud_remote)?;
    let folder = advanced::state_directory(db)?.join("activity");
    fs::create_dir_all(&folder)?;
    let (code, data) = fetch_small(
        o,
        &s,
        &["about", &format!("{name}:"), "--json"],
        &folder,
        &mut || Ok(()),
    )?;
    let value = if code == 0 {
        serde_json::from_slice::<Quota>(&data).unwrap_or(Quota {
            total: None,
            used: None,
            free: None,
        })
    } else {
        Quota {
            total: None,
            used: None,
            free: None,
        }
    };
    db.execute("INSERT INTO cloud_capacity VALUES(?1,?2,?3,?4,?5) ON CONFLICT(job_id) DO UPDATE SET queried_at=excluded.queried_at,total=excluded.total,used=excluded.used,free=excluded.free",params![job.id,backup::now(),value.total,value.used,value.free])?;
    Ok(value)
}
fn curl(o: &Options, config: &str) -> Result<()> {
    let mut c = advanced::command(&o.curl_executable)?;
    c.args([
        "--disable",
        "--config",
        "-",
        "--silent",
        "--show-error",
        "--fail",
        "--connect-timeout",
        "10",
        "--max-time",
        "30",
        "--proto",
        "=https,smtps,smtp",
    ])
    .stdin(Stdio::piped());
    let mut child = c.spawn()?;
    child
        .stdin
        .take()
        .ok_or_else(|| advanced::error("notification_unavailable"))?
        .write_all(config.as_bytes())?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(advanced::error("notification_unavailable"))
            };
        }
        if start.elapsed() > Duration::from_secs(35) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(advanced::error("notification_unavailable"));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
struct NotificationReport {
    subject: String,
    body: String,
}
fn report_size(bytes: i64, ru: bool) -> String {
    let labels = if ru {
        ["Б", "КБ", "МБ", "ГБ", "ТБ"]
    } else {
        ["B", "KB", "MB", "GB", "TB"]
    };
    let mut size = bytes.max(0) as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < 4 {
        size /= 1024.0;
        unit += 1;
    }
    let number = if unit == 0 {
        format!("{size:.0}")
    } else {
        format!("{size:.1}")
    };
    format!(
        "{} {}",
        if ru { number.replace('.', ",") } else { number },
        labels[unit]
    )
}
fn notification_report(
    language: &str,
    name: &str,
    outcome: &str,
    bytes: i64,
    seconds: i64,
    finished: i64,
) -> NotificationReport {
    let ru = language != "en";
    let name = name.replace(['\r', '\n'], " ");
    let success = outcome == "succeeded";
    let status = if success {
        if ru {
            "Успешно"
        } else {
            "Succeeded"
        }
    } else if outcome == "cancelled" {
        if ru {
            "Отменено пользователем"
        } else {
            "Cancelled by user"
        }
    } else if ["stale_preview", "file_changed_during_copy"].contains(&outcome) {
        if ru {
            "Не завершено: файлы изменились"
        } else {
            "Not completed: source files changed"
        }
    } else {
        if ru {
            "Не завершено"
        } else {
            "Not completed"
        }
    };
    let seconds = seconds.max(0);
    let duration = if ru {
        format!(
            "{} ч {} мин {} с",
            seconds / 3600,
            seconds % 3600 / 60,
            seconds % 60
        )
    } else {
        format!(
            "{} h {} min {} s",
            seconds / 3600,
            seconds % 3600 / 60,
            seconds % 60
        )
    };
    let date = chrono::DateTime::from_timestamp(finished, 0)
        .map(|v| {
            v.with_timezone(&chrono::Local)
                .format(if ru {
                    "%d.%m.%Y %H:%M:%S %:z"
                } else {
                    "%Y-%m-%d %H:%M:%S %:z"
                })
                .to_string()
        })
        .unwrap_or_else(|| "—".into());
    let subject = if ru {
        format!(
            "NAS Backup Manager: задача «{name}» {}",
            if success {
                "успешно"
            } else {
                "не завершено"
            }
        )
    } else {
        format!(
            "NAS Backup Manager: job “{name}” {}",
            if success {
                "succeeded"
            } else {
                "not completed"
            }
        )
    };
    let size = report_size(bytes, ru);
    let body = if ru {
        format!("NAS Backup Manager\nЗадача: {name}\nСтатус: {status}\nОбъём синхронизированных данных: {size}\nВремя выполнения: {duration}\nДата синхронизации: {date}")
    } else {
        format!("NAS Backup Manager\nJob: {name}\nStatus: {status}\nSynchronized data: {size}\nDuration: {duration}\nSynchronization date: {date}")
    };
    NotificationReport { subject, body }
}
fn formatted_report(report: &NotificationReport) -> String {
    fn escape(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
    let mut text = String::from("<b>NAS Backup Manager</b>\n");
    for line in report.body.lines().skip(1) {
        text.push('\n');
        if let Some((label, value)) = line.split_once(": ") {
            text.push_str(&format!("<b>{}:</b> {}", escape(label), escape(value)));
        } else {
            text.push_str(&escape(line));
        }
    }
    text
}
fn telegram_payload(chat: &str, report: &NotificationReport) -> serde_json::Value {
    serde_json::json!({"chat_id": chat, "text": formatted_report(report), "parse_mode": "HTML"})
}
fn mail_message(from: &str, to: &str, report: &NotificationReport) -> String {
    let boundary = format!("nbm-{}", uuid::Uuid::new_v4());
    let html = format!(
        "<!doctype html><html><body>{}</body></html>",
        formatted_report(report).replace('\n', "<br>\n")
    );
    format!("From: {from}\r\nTo: {to}\r\nSubject: {}\r\nMIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary=\"{boundary}\"\r\n\r\n--{boundary}\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\n{}\r\n--{boundary}\r\nContent-Type: text/html; charset=UTF-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\n{}\r\n--{boundary}--\r\n", mail_subject(&report.subject), mail_body(&report.body), mail_body(&html))
}
// RFC 2047 Q-encoding: each word is <= 75 ASCII bytes and ends at a UTF-8 boundary.
fn mail_subject(subject: &str) -> String {
    let mut words = Vec::new();
    let mut word = String::new();
    for ch in subject.replace(['\r', '\n'], " ").chars() {
        let mut bytes = [0u8; 4];
        let text = ch.encode_utf8(&mut bytes);
        if word.len() + text.len() * 3 > 60 && !word.is_empty() {
            words.push(format!("=?UTF-8?Q?{word}?="));
            word.clear();
        }
        for byte in text.as_bytes() {
            use std::fmt::Write;
            let _ = write!(word, "={byte:02X}");
        }
    }
    if !word.is_empty() {
        words.push(format!("=?UTF-8?Q?{word}?="));
    }
    words.join("\r\n ")
}
pub fn notify(
    db: &Connection,
    job: &SavedJob,
    outcome: &str,
    run: Option<&str>,
    started: i64,
) -> Result<()> {
    let o = &job.advanced;
    if !o.telegram_enabled && !o.smtp_enabled {
        return Ok(());
    }
    let (effective, s) = crate::notification_settings::effective(db, o)?;
    let o = &effective;
    use rusqlite::OptionalExtension;
    let language: String =
        db.query_row("SELECT language FROM settings WHERE singleton=1", [], |r| {
            r.get(0)
        })?;
    let metrics = db.query_row("SELECT coalesce((SELECT sum(byte_count) FROM operations WHERE run_id=r.id AND state='committed' AND kind IN ('create','update')),0),r.started_at,r.finished_at FROM runs r WHERE r.id=?1 AND r.job_id=?2 AND r.trigger_kind!='preview'",params![run,job.id],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,Option<i64>>(2)?))).optional()?;
    let (bytes, started, finished) = metrics
        .map(|(b, s, f)| (b, s, f.unwrap_or_else(backup::now)))
        .unwrap_or((0, started, backup::now()));
    let report = notification_report(
        &language,
        &job.name,
        outcome,
        bytes,
        finished.saturating_sub(started),
        finished,
    );
    let mut failed = false;
    if o.telegram_enabled {
        let json = telegram_payload(&o.telegram_chat, &report);
        let config = format!(
            "url = {}\nheader = {}\ndata = {}\n",
            quote(&format!(
                "https://api.telegram.org/bot{}/sendMessage",
                s.telegram_token
            )),
            quote("Content-Type: application/json"),
            quote(&json.to_string())
        );
        failed |= curl(o, &config).is_err();
    }
    if o.smtp_enabled {
        let body = advanced::state_directory(db)?.join(format!("{}.mail", uuid::Uuid::new_v4()));
        let content = mail_message(&o.smtp_from, &o.smtp_to, &report);
        fs::write(&body, content)?;
        let config = format!(
            "url = {}\nuser = {}\nmail-from = {}\nmail-rcpt = {}\nupload-file = {}\nssl-reqd\n",
            quote(&o.smtp_url),
            quote(&format!("{}:{}", o.smtp_username, s.smtp_password)),
            quote(&o.smtp_from),
            quote(&o.smtp_to),
            quote(&body.to_string_lossy())
        );
        let result = curl(o, &config);
        let _ = fs::remove_file(body);
        failed |= result.is_err();
    }
    if failed {
        return Err(advanced::error("notification_unavailable"));
    }
    Ok(())
}
pub fn report_notifications(
    db: &Connection,
    job: &SavedJob,
    outcome: &str,
    run: Option<&str>,
    started: i64,
) {
    if let Err(e) = notify(db, job, outcome, run, started) {
        let _=db.execute("INSERT INTO audit_events(id,job_id,kind,happened_at,state,error_code) VALUES(?1,?2,'notification',?3,'failed',?4)",params![uuid::Uuid::new_v4().to_string(),job.id,backup::now(),e.code]);
    }
}

/// Export an encrypted current file without overwriting a user-selected target.
pub fn decrypt_file(
    db: &Connection,
    job: &SavedJob,
    relative: &str,
    directory: &Path,
) -> Result<()> {
    let input = backup::safe_join(&backup::current(job), relative)?;
    decrypt_source(db, job, &input, relative, directory)
}
pub fn decrypt_source(
    db: &Connection,
    job: &SavedJob,
    input: &Path,
    relative: &str,
    directory: &Path,
) -> Result<()> {
    let s = advanced::secrets(db, &job.advanced)?;
    if s.age_identity.is_empty() {
        return Err(advanced::error("secret_required"));
    }
    let directory = backup::directory(directory)?;
    if directory.starts_with(&job.source) || directory.starts_with(&job.destination) {
        return Err(advanced::error("overlap"));
    }
    let name = Path::new(relative)
        .file_name()
        .ok_or_else(|| advanced::error("invalid_path"))?;
    let target = directory.join(name);
    let temp = directory.join(format!("{}.decrypting", uuid::Uuid::new_v4()));
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let result = (|| -> Result<()> {
        let mut cmd = advanced::command(&job.advanced.age_executable)?;
        cmd.args(["--decrypt", "--identity", "-"])
            .arg(input)
            .stdin(Stdio::piped())
            .stdout(Stdio::from(file));
        let mut child = cmd.spawn()?;
        child
            .stdin
            .take()
            .ok_or_else(|| advanced::error("encryption_failed"))?
            .write_all(s.age_identity.as_bytes())?;
        let code = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        if !code.success() {
            return Err(advanced::error("encryption_failed"));
        }
        fs::OpenOptions::new().write(true).open(&temp)?.sync_all()?;
        fs::hard_link(&temp, &target).map_err(|_| advanced::error("restore_collision"))?;
        Ok(())
    })();
    let _ = fs::remove_file(temp);
    result
}

// Encode every UTF-8 byte and fold safely: even older SMTP relays remain 7-bit clean.
fn mail_body(message: &str) -> String {
    let normalized = message.replace('\n', "\r\n");
    let mut out = String::new();
    for chunk in normalized.as_bytes().chunks(24) {
        if !out.is_empty() {
            out.push_str("=\r\n");
        }
        for byte in chunk {
            use std::fmt::Write;
            let _ = write!(out, "={byte:02X}");
        }
    }
    out
}
#[cfg(test)]
mod tests {

    #[test]
    fn email_has_formatted_html_plain_fallback_and_original_subject() {
        let report = super::notification_report(
            "ru",
            "A <test> & backup",
            "succeeded",
            0,
            12,
            1_792_800_000,
        );
        let mail = super::mail_message("from@example.invalid", "to@example.invalid", &report);
        assert!(mail.contains(&format!(
            "Subject: {}\r\n",
            super::mail_subject(&report.subject)
        )));
        let (headers, body) = mail.split_once("\r\n\r\n").unwrap();
        let boundary = headers
            .split("boundary=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        let parts: Vec<_> = body.split(&format!("--{boundary}")).collect();
        assert_eq!(parts.len(), 4);
        assert!(parts[1].contains("Content-Type: text/plain; charset=UTF-8"));
        assert!(parts[1].contains(&super::mail_body(&report.body)));
        assert!(parts[2].contains("Content-Type: text/html; charset=UTF-8"));
        let encoded = parts[2]
            .split_once("\r\n\r\n")
            .unwrap()
            .1
            .trim_end_matches("\r\n")
            .replace("=\r\n", "");
        let decoded: Vec<u8> = encoded
            .as_bytes()
            .chunks(3)
            .map(|v| u8::from_str_radix(std::str::from_utf8(&v[1..]).unwrap(), 16).unwrap())
            .collect();
        let html = String::from_utf8(decoded).unwrap().replace("\r\n", "\n");
        assert!(html.contains(
            "<b>NAS Backup Manager</b><br>\n<br>\n<b>Задача:</b> A &lt;test&gt; &amp; backup"
        ));
        assert!(html.contains("<b>Время выполнения:</b> 0 ч 0 мин 12 с"));
        assert_eq!(parts[3], "--\r\n");
    }

    #[test]
    fn telegram_report_formats_labels_and_escapes_job_names() {
        let report =
            super::notification_report("ru", "проверка", "succeeded", 0, 12, 1_792_800_000);
        let payload = super::telegram_payload("123", &report);
        assert_eq!(payload["chat_id"], "123");
        assert_eq!(payload["parse_mode"], "HTML");
        let date = report
            .body
            .lines()
            .last()
            .unwrap()
            .strip_prefix("Дата синхронизации: ")
            .unwrap();
        assert_eq!(payload["text"], format!("<b>NAS Backup Manager</b>\n\n<b>Задача:</b> проверка\n<b>Статус:</b> Успешно\n<b>Объём синхронизированных данных:</b> 0 Б\n<b>Время выполнения:</b> 0 ч 0 мин 12 с\n<b>Дата синхронизации:</b> {date}"));
        let report = super::notification_report(
            "en",
            "A & <b>test</b>\nNext",
            "cloud_failed",
            1024,
            12,
            1_792_800_000,
        );
        let payload = super::telegram_payload("123", &report);
        let text = payload["text"].as_str().unwrap();
        assert!(text.contains("<b>Job:</b> A &amp; &lt;b&gt;test&lt;/b&gt; Next"));
        assert!(text.contains("<b>Status:</b> Not completed"));
        assert!(text.contains("<b>Duration:</b> 0 h 0 min 12 s"));
        assert!(!text.contains(&report.subject));
    }

    #[test]
    fn reports_localize_outcomes_sizes_duration_and_finish_date() {
        let finished = 1_792_800_000;
        let ru = super::notification_report(
            "ru",
            "Документы",
            "succeeded",
            5 * 1024 * 1024,
            3661,
            finished,
        );
        assert_eq!(ru.subject, "NAS Backup Manager: задача «Документы» успешно");
        assert!(ru.body.contains("Задача: Документы\nСтатус: Успешно"));
        assert!(ru.body.contains("5,0 МБ"));
        assert!(ru.body.contains("1 ч 1 мин 1 с"));
        let date = chrono::DateTime::from_timestamp(finished, 0)
            .unwrap()
            .with_timezone(&chrono::Local)
            .format("%d.%m.%Y %H:%M:%S %:z")
            .to_string();
        assert!(ru.body.ends_with(&date));
        let en = super::notification_report("en", "Documents", "cloud_failed", 1024, 12, finished);
        assert_eq!(
            en.subject,
            "NAS Backup Manager: job “Documents” not completed"
        );
        assert!(en.body.contains("Job: Documents\nStatus: Not completed"));
        assert!(en.body.contains("1.0 KB"));
        assert!(en.body.contains("0 h 0 min 12 s"));
        for (bytes, unit) in [
            (0, "B"),
            (1023, "B"),
            (1024, "KB"),
            (1024_i64.pow(2), "MB"),
            (1024_i64.pow(3), "GB"),
            (1024_i64.pow(4), "TB"),
        ] {
            assert!(super::report_size(bytes, false).ends_with(unit));
        }
        let cancelled = super::notification_report("ru", "Task", "cancelled", 0, 60, finished);
        assert!(cancelled.body.contains("Отменено пользователем"));
        assert!(cancelled.body.contains("0 Б"));
        assert!(cancelled.body.contains("0 ч 1 мин 0 с"));
    }
    #[test]
    fn encoded_mail_subject_roundtrips_unicode_without_header_injection() {
        let subject = "NAS Backup Manager: задача «Документы 🗂» успешно";
        let encoded = super::mail_subject(subject);
        let mut decoded = String::new();
        for word in encoded.split("\r\n ") {
            assert!(word.len() <= 75);
            let text = word
                .strip_prefix("=?UTF-8?Q?")
                .unwrap()
                .strip_suffix("?=")
                .unwrap();
            let bytes: Vec<u8> = text
                .as_bytes()
                .chunks(3)
                .map(|v| u8::from_str_radix(std::str::from_utf8(&v[1..]).unwrap(), 16).unwrap())
                .collect();
            decoded.push_str(&String::from_utf8(bytes).unwrap());
        }
        assert_eq!(decoded, subject);
        let unsafe_report =
            super::notification_report("ru", "Name\r\nBcc: stranger", "succeeded", 0, 0, 0);
        assert!(!unsafe_report.subject.contains(['\r', '\n']));
        assert!(!super::mail_subject(&unsafe_report.subject).contains("Bcc:"));
    }
    #[test]
    fn cloud_stats_stream_is_bounded_and_uses_actual_transfer_values() {
        let latest = std::sync::Mutex::new(None);
        let mut buffer = Vec::new();
        let mut skip = false;
        let json = br#"{"stats":{"bytes":200,"totalBytes":1000,"transfers":1,"totalTransfers":4,"speed":100,"eta":8,"transferring":[{"name":"nested/file.bin"}]}}"#;
        super::progress_line(&mut buffer, &mut skip, &vec![b'x'; 70000], &latest);
        assert!(buffer.capacity() <= 65536);
        assert!(skip);
        super::progress_line(&mut buffer, &mut skip, b"\n", &latest);
        for chunk in json.chunks(7) {
            super::progress_line(&mut buffer, &mut skip, chunk, &latest);
        }
        super::progress_line(&mut buffer, &mut skip, b"\n", &latest);
        let sample = latest.lock().unwrap().take().unwrap();
        assert_eq!(
            (
                sample.bytes,
                sample.total_bytes,
                sample.completed,
                sample.total
            ),
            (200, 1000, 1, 4)
        );
        assert_eq!(sample.eta_seconds, Some(8));
        assert_eq!(sample.current_path.as_deref(), Some("nested/file.bin"));
        assert!(super::cloud_stats(br#"{"msg":"private unrelated log"}"#).is_none());
        let ending = super::cloud_stats(
            br#"{"stats":{"bytes":1000,"totalBytes":1000,"transfers":4,"totalTransfers":4}}"#,
        )
        .unwrap();
        assert!(ending.eta_seconds.is_none());
        let measured =
            super::cloud_stats(br#"{"stats":{"bytes":200,"totalBytes":1000,"elapsedTime":4}}"#)
                .unwrap();
        assert_eq!(measured.eta_seconds, Some(16));
    }
    #[test]
    #[ignore = "requires explicit rclone; streams only disposable local data"]
    fn rclone_transfer_reports_intermediate_bytes_eta_and_cancels() {
        let t = tempfile::tempdir().unwrap();
        let source = t.path().join("source");
        let dest = t.path().join("destination");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("large.bin"), vec![42; 4 * 1024 * 1024]).unwrap();
        let tool = std::env::var("NBM_TEST_RCLONE").unwrap();
        let config = t.path().join("empty.conf");
        std::fs::write(&config, b"").unwrap();
        let o = crate::advanced::Options {
            rclone_executable: tool,
            rclone_config: config.to_string_lossy().into(),
            ..Default::default()
        };
        let mut samples = Vec::new();
        super::transfer(
            || {
                let mut c = super::rclone(&o, &Default::default())?;
                c.arg("copy")
                    .arg(&source)
                    .arg(&dest)
                    .args(["--bwlimit", "1M"]);
                Ok(c)
            },
            &o,
            &mut || Ok(()),
            &mut |p| {
                samples.push(p);
                Ok(())
            },
        )
        .unwrap();
        assert!(samples
            .iter()
            .any(|p| p.bytes > 0 && p.bytes < p.total_bytes));
        assert!(samples.iter().any(|p| p.eta_seconds.is_some()));
        assert_eq!(samples.last().unwrap().bytes, 4 * 1024 * 1024);
        assert_eq!(
            std::fs::read(dest.join("large.bin")).unwrap(),
            std::fs::read(source.join("large.bin")).unwrap()
        );
        let cancelled_dest = t.path().join("cancelled");
        let started = std::time::Instant::now();
        let e = super::transfer(
            || {
                let mut c = super::rclone(&o, &Default::default())?;
                c.arg("copy")
                    .arg(&source)
                    .arg(&cancelled_dest)
                    .args(["--bwlimit", "1M"]);
                Ok(c)
            },
            &o,
            &mut || Ok(()),
            &mut |p| {
                if p.bytes > 0 {
                    Err(crate::advanced::error("cancelled"))
                } else {
                    Ok(())
                }
            },
        )
        .unwrap_err();
        assert_eq!(e.code, "cancelled");
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
    }

    #[test]
    fn unsupported_cloud_is_rejected_before_starting_a_process() {
        let options = crate::advanced::Options {
            cloud_provider: "retired-provider".into(),
            cloud_mode: "replica".into(),
            ..Default::default()
        };
        assert_eq!(
            super::rclone(&options, &Default::default())
                .err()
                .unwrap()
                .code,
            "cloud_provider_unavailable"
        );
        assert_eq!(
            crate::advanced::validate(&options).unwrap_err().code,
            "cloud_provider_unavailable"
        );
        let disabled = crate::advanced::Options {
            cloud_mode: "off".into(),
            ..options
        };
        crate::advanced::validate(&disabled).unwrap();
    }
    #[test]
    fn remote_diagnostics_use_only_fixed_codes() {
        assert_eq!(
            super::classify_remote_error(b"[410 - GoneError] token=never-show-this"),
            Some("cloud_api_unavailable")
        );
        assert_eq!(
            super::classify_remote_error(b"401 Unauthorized refresh_token=private"),
            Some("cloud_authentication_failed")
        );
        assert_eq!(
            super::classify_remote_error(b"[403 - ForbiddenError]"),
            Some("cloud_permission_denied")
        );
        assert_eq!(
            super::classify_remote_error(b"unrelated filename 410.txt"),
            None
        );
    }
    #[cfg(windows)]
    #[test]
    fn process_error_output_is_redacted_and_drained() {
        let path = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let mut cmd = crate::advanced::command(path.to_str().unwrap()).unwrap();
        cmd.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Console]::Error.WriteLine('[410 - GoneError] private-fixture-value'); exit 1",
        ]);
        let failure =
            super::run(cmd, std::time::Duration::from_secs(5), &mut || Ok(())).unwrap_err();
        assert_eq!(failure.code, "cloud_api_unavailable");
        assert_eq!(failure.detail, "");
    }
    #[test]
    fn mail_encoding_preserves_unicode_and_line_limits() {
        let original = "Задача · Отчёт\nУспешно: 1024 байта";
        let encoded = super::mail_body(original);
        assert!(encoded.lines().all(|line| line.len() <= 73));
        let joined = encoded.replace("=\r\n", "");
        let decoded: Vec<u8> = joined
            .as_bytes()
            .chunks(3)
            .map(|part| u8::from_str_radix(std::str::from_utf8(&part[1..]).unwrap(), 16).unwrap())
            .collect();
        assert_eq!(
            String::from_utf8(decoded).unwrap(),
            original.replace('\n', "\r\n")
        );
    }

    #[test]
    fn curl_config_escaping_never_creates_a_second_directive() {
        assert_eq!(super::quote("a\"\nb\\c"), "\"a\\\"\\nb\\\\c\"");
    }
}
