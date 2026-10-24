//! Optional job policies. No resident workers, no network activity while idle.
use crate::backup::{Failure, Result, SavedJob};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub(crate) fn error(code: &str) -> Failure {
    Failure {
        code: code.into(),
        detail: String::new(),
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    pub verify_enabled: bool,
    pub verify_every_days: u32,
    pub verify_mib_per_second: u32,
    pub retry_attempts: u32,
    pub retry_initial_seconds: u32,
    pub retry_max_seconds: u32,
    pub bandwidth_kib: u32,
    pub bandwidth_start_minute: u16,
    pub bandwidth_end_minute: u16,
    pub encryption_recipient: String,
    pub age_executable: String,
    pub cloud_mode: String,
    pub cloud_remote: String,
    pub cloud_connection: String,
    pub cloud_provider: String,
    pub rclone_executable: String,
    pub rclone_config: String,
    pub telegram_enabled: bool,
    pub telegram_chat: String,
    pub smtp_enabled: bool,
    pub smtp_url: String,
    pub smtp_username: String,
    pub smtp_from: String,
    pub smtp_to: String,
    pub curl_executable: String,
    pub vault_id: String,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            verify_enabled: false,
            verify_every_days: 7,
            verify_mib_per_second: 8,
            retry_attempts: 3,
            retry_initial_seconds: 2,
            retry_max_seconds: 60,
            bandwidth_kib: 0,
            bandwidth_start_minute: 540,
            bandwidth_end_minute: 1080,
            encryption_recipient: String::new(),
            age_executable: String::new(),
            cloud_mode: "off".into(),
            cloud_remote: String::new(),
            cloud_connection: String::new(),
            cloud_provider: String::new(),
            rclone_executable: String::new(),
            rclone_config: String::new(),
            telegram_enabled: false,
            telegram_chat: String::new(),
            smtp_enabled: false,
            smtp_url: "smtps://smtp.example.com:465".into(),
            smtp_username: String::new(),
            smtp_from: String::new(),
            smtp_to: String::new(),
            curl_executable: String::new(),
            vault_id: String::new(),
        }
    }
}
/// Debug output deliberately excludes all secret values.
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Secrets {
    pub telegram_token: String,
    pub smtp_password: String,
    pub rclone_password: String,
    pub age_identity: String,
}
impl std::fmt::Debug for Secrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secrets([REDACTED])")
    }
}
impl Secrets {
    fn merge(&mut self, update: &Self) {
        for (old, new) in [
            (&mut self.telegram_token, &update.telegram_token),
            (&mut self.smtp_password, &update.smtp_password),
            (&mut self.rclone_password, &update.rclone_password),
            (&mut self.age_identity, &update.age_identity),
        ] {
            if !new.is_empty() {
                *old = new.clone();
            }
        }
    }
}

pub fn validate(options: &Options) -> Result<()> {
    if options.verify_every_days == 0
        || options.retry_attempts > 20
        || options.retry_initial_seconds == 0
        || options.retry_max_seconds < options.retry_initial_seconds
        || options.retry_max_seconds > 3600
        || options.bandwidth_start_minute >= 1440
        || options.bandwidth_end_minute >= 1440
        || !["off", "primary", "replica"].contains(&options.cloud_mode.as_str())
    {
        return Err(error("invalid_advanced"));
    }
    if !options.encryption_recipient.is_empty()
        && (!options.encryption_recipient.starts_with("age1")
            || options.encryption_recipient.len() != 62
            || !options
                .encryption_recipient
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()))
    {
        return Err(error("invalid_encryption_key"));
    }
    if options.cloud_mode != "off" {
        if !options.cloud_provider.is_empty()
            && !["onedrive", "dropbox"].contains(&options.cloud_provider.as_str())
        {
            return Err(error("cloud_provider_unavailable"));
        }
        remote_parts(&options.cloud_remote)?;
        let p = Path::new(&options.rclone_config);
        if !p.is_absolute()
            || fs::metadata(p)?.len() > 1024 * 1024
            || !fs::read_to_string(p)?
                .lines()
                .any(|line| line.trim() == "RCLONE_ENCRYPT_V0:")
        {
            return Err(error("rclone_config_not_encrypted"));
        }
    }
    for (enabled, executable) in [
        (options.cloud_mode != "off", &options.rclone_executable),
        (
            !options.encryption_recipient.is_empty(),
            &options.age_executable,
        ),
    ] {
        if enabled {
            executable_path(executable)?;
        }
    }
    Ok(())
}
pub(crate) fn validate_notification_options(options: &Options) -> Result<()> {
    for text in [
        &options.smtp_from,
        &options.smtp_to,
        &options.smtp_username,
        &options.telegram_chat,
    ] {
        if text.contains(['\r', '\n', '\0']) {
            return Err(error("invalid_advanced"));
        }
    }
    if options.smtp_enabled
        && (!options.smtp_url.starts_with("smtps://") && !options.smtp_url.starts_with("smtp://")
            || options.smtp_url.contains(['@', '?', '#', '\r', '\n', '\0'])
            || !options.smtp_from.contains('@')
            || !options.smtp_to.contains('@'))
    {
        return Err(error("invalid_advanced"));
    }
    if options.telegram_enabled
        && (options.telegram_chat.is_empty()
            || !options.telegram_chat.bytes().all(|b| {
                b.is_ascii_digit() || b == b'-' || b == b'@' || b == b'_' || b.is_ascii_alphabetic()
            }))
    {
        return Err(error("invalid_advanced"));
    }
    if options.telegram_enabled || options.smtp_enabled {
        executable_path(&options.curl_executable)?;
    }
    Ok(())
}
pub fn executable_path(value: &str) -> Result<&Path> {
    let p = Path::new(value);
    if !p.is_absolute() || !fs::symlink_metadata(p).is_ok_and(|m| m.is_file()) {
        return Err(error("tool_unavailable"));
    }
    Ok(p)
}
pub fn state_directory(db: &Connection) -> Result<PathBuf> {
    let path: String = db.query_row(
        "SELECT file FROM pragma_database_list WHERE name='main'",
        [],
        |r| r.get(0),
    )?;
    Path::new(&path)
        .parent()
        .filter(|_| !path.is_empty())
        .map(Path::to_owned)
        .ok_or_else(|| error("state_directory_unavailable"))
}
pub(crate) fn vault_path(db: &Connection, id: &str) -> Result<PathBuf> {
    crate::model::JobId::parse(id).map_err(|_| error("vault_unavailable"))?;
    let folder = state_directory(db)?.join("secrets");
    fs::create_dir_all(&folder)?;
    if fs::symlink_metadata(&folder)?.file_type().is_symlink() {
        return Err(error("vault_unavailable"));
    }
    Ok(folder.join(format!("{id}.dpapi")))
}
pub fn secrets(db: &Connection, options: &Options) -> Result<Secrets> {
    let mut result: Secrets = if options.vault_id.is_empty() {
        Secrets::default()
    } else {
        let bytes =
            nas_backup_storage::vault::load(&vault_path(db, &options.vault_id)?).map_err(error)?;
        serde_json::from_slice(&bytes).map_err(|_| error("vault_unavailable"))?
    };
    if options.cloud_mode != "off" && !options.cloud_connection.is_empty() {
        result.rclone_password = crate::cloud_setup::password(db, &options.cloud_connection)?;
    }
    Ok(result)
}
pub fn save_secrets(
    db: &Connection,
    options: &mut Options,
    old: Option<&Options>,
    update: Option<&Secrets>,
) -> Result<()> {
    let mut all = if let Some(old) = old {
        match secrets(db, old) {
            Ok(value) => value,
            Err(e) if update.is_some() => {
                let _ = e;
                Secrets::default()
            }
            Err(e) => return Err(e),
        }
    } else {
        Secrets::default()
    };
    if let Some(update) = update {
        all.merge(update);
    }
    if options.cloud_mode != "off" && !options.cloud_connection.is_empty() {
        crate::cloud_setup::validate(db, options)?;
        all.rclone_password = crate::cloud_setup::password(db, &options.cloud_connection)?;
    }
    if [
        &all.telegram_token,
        &all.smtp_password,
        &all.rclone_password,
    ]
    .iter()
    .any(|s| s.len() > 16384)
        || all.age_identity.len() > 65536
    {
        return Err(error("invalid_advanced"));
    }
    if options.telegram_enabled || options.smtp_enabled {
        crate::notification_settings::validate_job(db, options, &all)?;
    }
    if options.cloud_mode != "off" && all.rclone_password.is_empty() {
        return Err(error("secret_required"));
    }
    if all.telegram_token.is_empty()
        && all.smtp_password.is_empty()
        && all.rclone_password.is_empty()
        && all.age_identity.is_empty()
    {
        options.vault_id.clear();
        return Ok(());
    }
    if update.is_none() {
        if let Some(old) = old {
            options.vault_id = old.vault_id.clone();
            return Ok(());
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    nas_backup_storage::vault::save_new(
        &vault_path(db, &id)?,
        &serde_json::to_vec(&all).map_err(|_| error("vault_unavailable"))?,
    )
    .map_err(error)?;
    options.vault_id = id;
    Ok(())
}
pub fn remote_parts(value: &str) -> Result<(&str, &str)> {
    let (remote, path) = value
        .split_once(':')
        .ok_or_else(|| error("invalid_remote"))?;
    if remote.is_empty()
        || !remote
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        || path.is_empty()
        || path.starts_with('/')
        || path.ends_with('/')
        || path.split('/').any(|p| {
            p.is_empty() || p == "." || p == ".." || p.contains(['\\', ':', '\r', '\n', '\0'])
        })
    {
        return Err(error("invalid_remote"));
    }
    Ok((remote, path))
}
pub fn check_remote_isolation(
    db: &Connection,
    job_id: Option<&str>,
    options: &Options,
) -> Result<()> {
    if options.cloud_mode == "off" {
        return Ok(());
    }
    let (remote, path) = remote_parts(&options.cloud_remote)?;
    let mut q = db.prepare("SELECT id,advanced_json FROM jobs")?;
    for row in q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (id, json) = row?;
        if Some(id.as_str()) == job_id {
            continue;
        }
        let o: Options = serde_json::from_str(&json).map_err(|_| error("invalid_advanced"))?;
        if o.cloud_mode != "off" {
            let (r, p) = remote_parts(&o.cloud_remote)?;
            if remote.eq_ignore_ascii_case(r)
                && (path.eq_ignore_ascii_case(p)
                    || path.to_lowercase().starts_with(&(p.to_lowercase() + "/"))
                    || p.to_lowercase().starts_with(&(path.to_lowercase() + "/")))
            {
                return Err(error("overlap"));
            }
        }
    }
    Ok(())
}
pub fn retry_delay(options: &Options, attempt: u32) -> Duration {
    Duration::from_secs(
        u64::from(options.retry_initial_seconds)
            .saturating_mul(1u64.checked_shl(attempt).unwrap_or(u64::MAX))
            .min(u64::from(options.retry_max_seconds)),
    )
}
pub fn transient(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::Interrupted
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::NotConnected
            | std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::WouldBlock
    ) || cfg!(windows)
        && matches!(
            e.raw_os_error(),
            Some(53 | 54 | 59 | 64 | 67 | 121 | 1231 | 1232)
        )
}
pub fn wait(duration: Duration, check: &mut impl FnMut() -> Result<()>) -> Result<()> {
    let start = Instant::now();
    while start.elapsed() < duration {
        check()?;
        std::thread::sleep(
            (duration - start.elapsed().min(duration)).min(Duration::from_millis(100)),
        );
    }
    check()
}
pub fn bandwidth_active(start: u16, end: u16, minute: u16) -> bool {
    if start == end {
        true
    } else if start < end {
        minute >= start && minute < end
    } else {
        minute >= start || minute < end
    }
}
pub struct Rate {
    next: Instant,
}
impl Default for Rate {
    fn default() -> Self {
        Self {
            next: Instant::now(),
        }
    }
}
impl Rate {
    pub fn account(
        &mut self,
        bytes: usize,
        kib: u32,
        check: &mut impl FnMut() -> Result<()>,
    ) -> Result<()> {
        if kib == 0 {
            self.next = Instant::now();
            return Ok(());
        }
        let now = Instant::now();
        self.next =
            self.next.max(now) + Duration::from_secs_f64(bytes as f64 / (f64::from(kib) * 1024.));
        if self.next > now {
            wait(self.next - now, check)?;
        }
        Ok(())
    }
}
pub fn bandwidth(options: &Options) -> u32 {
    use chrono::Timelike;
    let now = chrono::Local::now();
    if bandwidth_active(
        options.bandwidth_start_minute,
        options.bandwidth_end_minute,
        (now.hour() * 60 + now.minute()) as u16,
    ) {
        options.bandwidth_kib
    } else {
        0
    }
}
pub fn command(exe: &str) -> Result<Command> {
    let mut cmd = Command::new(executable_path(exe)?);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    Ok(cmd)
}
fn finish(child: &mut std::process::Child, check: &mut impl FnMut() -> Result<()>) -> Result<()> {
    loop {
        if let Err(e) = check() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(error("external_tool_failed"))
            };
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Stream through age's reviewed format; private keys are not needed for backup.
pub fn encrypt(
    source: &Path,
    target: &Path,
    o: &Options,
    check: &mut impl FnMut(i64, i64, &str) -> Result<()>,
) -> Result<String> {
    let before = fs::metadata(source)?;
    let source_id = file_id::get_file_id(source)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let mut child = command(&o.age_executable)?
        .args(["--encrypt", "--recipient", &o.encryption_recipient, "--"])
        .arg(source)
        .stdout(Stdio::piped())
        .spawn()?;
    let result = (|| -> Result<String> {
        let mut input = child
            .stdout
            .take()
            .ok_or_else(|| error("encryption_failed"))?;
        let mut buf = [0u8; 262144];
        let mut bytes = 0i64;
        let mut rate = Rate::default();
        let mut hash = Sha256::new();
        loop {
            check(bytes.min(before.len() as i64), 0, "copying")?;
            let n = input.read(&mut buf)?;
            if n == 0 {
                break;
            }
            output.write_all(&buf[..n])?;
            hash.update(&buf[..n]);
            bytes += n as i64;
            rate.account(n, bandwidth(o), &mut || {
                check(bytes.min(before.len() as i64), 0, "copying")
            })?;
        }
        finish(&mut child, &mut || {
            check(before.len() as i64, 0, "flushing")
        })?;
        output.sync_all()?;
        let after = fs::metadata(source)?;
        if before.len() != after.len()
            || before.modified()? != after.modified()?
            || source_id != file_id::get_file_id(source)?
        {
            return Err(error("file_changed_during_copy"));
        }
        filetime::set_file_mtime(
            target,
            filetime::FileTime::from_last_modification_time(&before),
        )?;
        let expected = format!("{:x}", hash.finalize());
        let actual = digest_progress(target, 0, &mut |read| {
            check(
                before.len() as i64,
                ((read as i128 * before.len() as i128) / bytes.max(1) as i128) as i64,
                "verifying",
            )
        })?;
        if expected != actual {
            return Err(error("integrity_failed"));
        }
        Ok(actual)
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

pub fn digest(path: &Path, kib: u32, check: &mut impl FnMut() -> Result<()>) -> Result<String> {
    digest_progress(path, kib, &mut |_| check())
}
fn digest_progress(
    path: &Path,
    kib: u32,
    check: &mut impl FnMut(i64) -> Result<()>,
) -> Result<String> {
    let mut f = File::open(path)?;
    let mut buf = [0u8; 262144];
    let mut h = Sha256::new();
    let mut rate = Rate::default();
    let mut read = 0;
    loop {
        check(read)?;
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        read += n as i64;
        rate.account(n, kib, &mut || check(read))?;
    }
    check(read)?;
    Ok(format!("{:x}", h.finalize()))
}

pub fn record_file(
    db: &Connection,
    job: &SavedJob,
    relative: &str,
    s: &fs::Metadata,
    stored: &Path,
    hash: &str,
) -> Result<()> {
    let d = fs::metadata(stored)?;
    db.execute("INSERT INTO file_integrity(job_id,path,source_bytes,source_modified,stored_bytes,stored_modified,sha256,verified_at,codec) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(job_id,path) DO UPDATE SET source_bytes=excluded.source_bytes,source_modified=excluded.source_modified,stored_bytes=excluded.stored_bytes,stored_modified=excluded.stored_modified,sha256=excluded.sha256,verified_at=excluded.verified_at,codec=excluded.codec",params![job.id,relative,s.len()as i64,crate::backup::fingerprint_meta(s)?,d.len()as i64,crate::backup::fingerprint_meta(&d)?,hash,crate::backup::now(),job.advanced.encryption_recipient])?;
    Ok(())
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityReport {
    pub checked_files: u64,
    pub baseline_files: u64,
    pub checked_at: i64,
}
struct IntegrityProgress<'a> {
    db: &'a Connection,
    snapshot: crate::backup::Progress,
    last: Instant,
    started: Instant,
}
impl IntegrityProgress<'_> {
    fn report(&mut self, force: bool) -> Result<()> {
        if force || self.last.elapsed() >= Duration::from_millis(400) {
            let elapsed = self.started.elapsed().as_secs_f64();
            self.snapshot.eta_seconds = if self.snapshot.verified_bytes > 0 && elapsed >= 1.0 {
                Some(
                    ((self.snapshot.total_bytes - self.snapshot.verified_bytes).max(0) as f64
                        * elapsed
                        / self.snapshot.verified_bytes as f64)
                        .ceil() as i64,
                )
            } else {
                None
            };
            crate::activity::report(self.db, &self.snapshot)?;
            self.last = Instant::now();
        }
        Ok(())
    }
    fn hash(
        &mut self,
        path: &Path,
        label: &str,
        kib: u32,
        check: &mut impl FnMut() -> Result<()>,
    ) -> Result<String> {
        self.snapshot.current_path = Some(label.to_owned());
        self.report(false)?;
        let base = self.snapshot.verified_bytes;
        digest_progress(path, kib, &mut |read| {
            check()?;
            self.snapshot.verified_bytes = base.saturating_add(read);
            self.report(false)
        })
    }
    fn complete(&mut self) -> Result<()> {
        self.snapshot.completed += 1;
        self.report(false)
    }
}
pub fn verify_existing(
    db: &Connection,
    job: &SavedJob,
    check: &mut impl FnMut() -> Result<()>,
) -> Result<IntegrityReport> {
    let (total, bytes): (i64,i64) = db.query_row(
        "SELECT (SELECT count(*) FROM file_integrity WHERE job_id=?1)+(SELECT count(*) FROM archive_versions WHERE job_id=?1 AND state='verified'), (SELECT coalesce(sum(stored_bytes),0) FROM file_integrity WHERE job_id=?1)+(SELECT coalesce(sum(byte_count),0) FROM archive_versions WHERE job_id=?1 AND state='verified')",
        [&job.id], |r| Ok((r.get(0)?,r.get(1)?)))?;
    let (old_total, old_bytes): (i64,i64) = db.query_row(
        "SELECT count(*),coalesce(sum(bytes),0)*2 FROM scan_entries WHERE run_id=(SELECT id FROM runs WHERE job_id=?1 AND scan_complete=1 ORDER BY started_at DESC,rowid DESC LIMIT 1) AND side=1 AND is_dir=0 AND path NOT IN (SELECT path FROM file_integrity WHERE job_id=?1)",
        [&job.id], |r| Ok((r.get(0)?,r.get(1)?)))?;
    let mut progress = IntegrityProgress {
        db,
        snapshot: crate::backup::Progress {
            job_id: job.id.clone(),
            run_id: String::new(),
            completed: 0,
            total: total + old_total,
            copied_bytes: 0,
            verified_bytes: 0,
            eta_seconds: None,
            total_bytes: bytes + old_bytes,
            phase: "integrity".into(),
            current_path: None,
        },
        last: Instant::now(),
        started: Instant::now(),
    };
    progress.report(true)?;
    let result = (|| -> Result<IntegrityReport> {
        let mut checked = 0;
        let mut baseline = 0;
        let mut q =
            db.prepare("SELECT path,sha256 FROM file_integrity WHERE job_id=?1 ORDER BY path")?;
        for row in q.query_map([&job.id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            let (path, expected) = row?;
            check()?;
            crate::backup::verify_resources(db, job, false)?;
            let stored = crate::backup::safe_join(&crate::backup::current(job), &path)?;
            if progress.hash(
                &stored,
                &path,
                job.advanced.verify_mib_per_second.saturating_mul(1024),
                check,
            )? != expected
            {
                return Err(error("integrity_failed"));
            }
            checked += 1;
            progress.complete()?;
        }
        // Old jobs have no trusted checksum. Establish a baseline against the source,
        // never certify destination bytes simply because they can be read.
        let mut q=db.prepare("SELECT path FROM scan_entries WHERE run_id=(SELECT id FROM runs WHERE job_id=?1 AND scan_complete=1 ORDER BY started_at DESC,rowid DESC LIMIT 1) AND side=1 AND is_dir=0 AND path NOT IN (SELECT path FROM file_integrity WHERE job_id=?1)")?;
        for row in q.query_map([&job.id], |r| r.get::<_, String>(0))? {
            let path = row?;
            check()?;
            let stored = crate::backup::safe_join(&crate::backup::current(job), &path)?;
            let source = crate::backup::safe_join(Path::new(&job.source), &path)?;
            if source.is_file() && job.advanced.encryption_recipient.is_empty() {
                let before = fs::metadata(&source)?;
                let a = progress.hash(
                    &source,
                    &path,
                    job.advanced.verify_mib_per_second.saturating_mul(1024),
                    check,
                )?;
                let b = progress.hash(
                    &stored,
                    &path,
                    job.advanced.verify_mib_per_second.saturating_mul(1024),
                    check,
                )?;
                if a == b
                    && before.len() == fs::metadata(&source)?.len()
                    && before.modified()? == fs::metadata(&source)?.modified()?
                {
                    record_file(db, job, &path, &before, &stored, &b)?;
                    baseline += 1;
                }
            }
            progress.complete()?;
        }
        // Archives already have immutable checksums, independent of current/source.
        let mut q=db.prepare("SELECT stored_relative_path,content_hash FROM archive_versions WHERE job_id=?1 AND state='verified'")?;
        for row in q.query_map([&job.id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            let (path, expected) = row?;
            check()?;
            let path = crate::backup::safe_join(&crate::backup::root(job).join("archive"), &path)?;
            if progress.hash(
                &path,
                &path.to_string_lossy(),
                job.advanced.verify_mib_per_second.saturating_mul(1024),
                check,
            )? != expected
            {
                return Err(error("integrity_failed"));
            }
            checked += 1;
            progress.complete()?;
        }
        progress.report(true)?;
        if job.advanced.cloud_mode != "off" {
            progress.snapshot.phase = "cloudIntegrity".into();
            progress.snapshot.eta_seconds = None;
            crate::activity::report(db, &progress.snapshot)?;
        }
        crate::remote::verify_cloud(db, job, check)?;
        Ok(IntegrityReport {
            checked_files: checked,
            baseline_files: baseline,
            checked_at: crate::backup::now(),
        })
    })();
    let (state, code) = match &result {
        Ok(_) => ("succeeded", None),
        Err(e) => ("failed", Some(e.code.as_str())),
    };
    let count = result.as_ref().map(|r| r.checked_files).unwrap_or(0);
    db.execute("INSERT INTO integrity_runs VALUES(?1,?2,?3,?4,?5) ON CONFLICT(job_id) DO UPDATE SET checked_at=excluded.checked_at,checked_files=excluded.checked_files,state=excluded.state,error_code=excluded.error_code",params![job.id,crate::backup::now(),count as i64,state,code])?;
    result
}
pub fn verify_due(db: &Connection, job: &SavedJob) -> Result<bool> {
    if !job.advanced.verify_enabled {
        return Ok(false);
    }
    let last: Option<(i64, String)> = db
        .query_row(
            "SELECT checked_at,state FROM integrity_runs WHERE job_id=?1",
            [&job.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(last.is_none_or(|(t, s)| {
        s != "succeeded"
            || crate::backup::now() - t >= i64::from(job.advanced.verify_every_days) * 86400
    }))
}

/// Retry only a single staged file, resuming from the last fully written buffer.
pub fn copy_resume(
    source: &Path,
    target: &Path,
    o: &Options,
    notify: &mut impl FnMut(i64, i64, &str) -> Result<()>,
) -> Result<String> {
    copy_resume_inner(source, target, o, notify, &mut |_, _| Ok(()))
}
fn copy_resume_inner(
    source: &Path,
    target: &Path,
    o: &Options,
    notify: &mut impl FnMut(i64, i64, &str) -> Result<()>,
    fault: &mut impl FnMut(u64, &mut File) -> std::io::Result<()>,
) -> Result<String> {
    let before = fs::metadata(source)?;
    let before_id = file_id::get_file_id(source)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let target_id = file_id::get_file_id(target)?;
    let mut input = File::open(source)?;
    let mut offset = 0u64;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 262144];
    let mut retries = 0;
    let mut rate = Rate::default();
    loop {
        notify(offset as i64, 0, "copying")?;
        let step = (|| -> std::io::Result<usize> {
            fault(offset, &mut output)?;
            let n = input.read(&mut buf)?;
            output.write_all(&buf[..n])?;
            Ok(n)
        })();
        match step {
            Ok(0) => break,
            Ok(n) => {
                hash.update(&buf[..n]);
                offset += n as u64;
                retries = 0;
                rate.account(n, bandwidth(o), &mut || notify(offset as i64, 0, "copying"))?;
            }
            Err(e) => {
                if !transient(&e) || retries >= o.retry_attempts {
                    return Err(e.into());
                }
                wait(retry_delay(o, retries), &mut || {
                    notify(offset as i64, 0, "retrying")
                })?;
                retries += 1;
                let mut recovery_options = o.clone();
                recovery_options.retry_attempts = o.retry_attempts.saturating_sub(retries);
                recovery_options.retry_initial_seconds = retry_delay(o, retries).as_secs() as u32;
                let mut recovery_attempts = 0;
                let (new_input, new_output) = retry_operation(
                    &recovery_options,
                    &mut || {
                        recovery_attempts += 1;
                        let after = fs::metadata(source)?;
                        if before_id != file_id::get_file_id(source)?
                            || before.len() != after.len()
                            || before.modified()? != after.modified()?
                            || target_id != file_id::get_file_id(target)?
                        {
                            return Err(error("file_changed_during_copy"));
                        }
                        let mut input = File::open(source)?;
                        input.seek(SeekFrom::Start(offset))?;
                        let mut output = OpenOptions::new().write(true).open(target)?;
                        output.set_len(offset)?;
                        output.seek(SeekFrom::Start(offset))?;
                        Ok((input, output))
                    },
                    &mut || notify(offset as i64, 0, "retrying"),
                )?;
                retries += recovery_attempts - 1;
                input = new_input;
                output = new_output;
            }
        }
    }
    notify(offset as i64, 0, "flushing")?;
    output.sync_all()?;
    drop(output);
    if before.len() != fs::metadata(source)?.len()
        || before.modified()? != fs::metadata(source)?.modified()?
        || before_id != file_id::get_file_id(source)?
    {
        return Err(error("file_changed_during_copy"));
    }
    filetime::set_file_mtime(
        target,
        filetime::FileTime::from_last_modification_time(&before),
    )?;
    let checksum = format!("{:x}", hash.finalize());
    if readback(target, o, offset as i64, notify)? != checksum {
        return Err(error("integrity_failed"));
    }
    notify(offset as i64, offset as i64, "verifying")?;
    Ok(checksum)
}

// Bounded metadata batches, never file contents; no transaction is held during I/O.
pub struct FileRecord {
    bytes: i64,
    modified: String,
    stored_bytes: i64,
    stored_modified: String,
    hash: String,
}
pub fn capture_record(
    db: &Connection,
    job: &SavedJob,
    run: &str,
    relative: &str,
    stored: &Path,
    hash: &str,
) -> Result<FileRecord> {
    let (bytes, modified) = db.query_row(
        "SELECT bytes,modified FROM scan_entries WHERE run_id=?1 AND side=0 AND path=?2",
        params![run, relative],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let _ = job;
    let m = fs::metadata(stored)?;
    Ok(FileRecord {
        bytes,
        modified,
        stored_bytes: m.len() as i64,
        stored_modified: crate::backup::fingerprint_meta(&m)?,
        hash: hash.into(),
    })
}
pub fn flush_records(
    db: &Connection,
    job: &SavedJob,
    pending: &mut Vec<(String, FileRecord)>,
) -> Result<()> {
    if pending.is_empty() {
        return Ok(());
    }
    let tx = db.unchecked_transaction()?;
    for (path, r) in pending.iter() {
        tx.execute("INSERT INTO file_integrity(job_id,path,source_bytes,source_modified,stored_bytes,stored_modified,sha256,verified_at,codec) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(job_id,path) DO UPDATE SET source_bytes=excluded.source_bytes,source_modified=excluded.source_modified,stored_bytes=excluded.stored_bytes,stored_modified=excluded.stored_modified,sha256=excluded.sha256,verified_at=excluded.verified_at,codec=excluded.codec",params![job.id,path,r.bytes,r.modified,r.stored_bytes,r.stored_modified,r.hash,crate::backup::now(),job.advanced.encryption_recipient])?;
    }
    tx.commit()?;
    pending.clear();
    Ok(())
}

pub fn retry_operation<T>(
    o: &Options,
    operation: &mut impl FnMut() -> Result<T>,
    check: &mut impl FnMut() -> Result<()>,
) -> Result<T> {
    for attempt in 0..=o.retry_attempts {
        check()?;
        match operation() {
            Ok(value) => return Ok(value),
            Err(e) => {
                if attempt == o.retry_attempts || !e.detail.starts_with("transient:") {
                    return Err(e);
                }
                wait(retry_delay(o, attempt), check)?;
            }
        }
    }
    Err(error("filesystem"))
}

fn readback(
    path: &Path,
    o: &Options,
    total: i64,
    notify: &mut impl FnMut(i64, i64, &str) -> Result<()>,
) -> Result<String> {
    for attempt in 0..=o.retry_attempts {
        match digest_progress(path, 0, &mut |read| notify(total, read, "verifying")) {
            Ok(value) => return Ok(value),
            Err(e) => {
                if attempt == o.retry_attempts || !e.detail.starts_with("transient:") {
                    return Err(e);
                }
                wait(retry_delay(o, attempt), &mut || {
                    notify(total, 0, "retrying")
                })?;
            }
        }
    }
    Err(error("filesystem"))
}

#[cfg(test)]
mod retry_tests {
    use super::*;
    #[test]
    fn partially_written_buffer_is_truncated_and_resumed_once() {
        let t = tempfile::tempdir().unwrap();
        let source = t.path().join("source");
        let target = t.path().join("target");
        let bytes = vec![42; 1024 * 1024];
        fs::write(&source, &bytes).unwrap();
        let mut injected = false;
        let o = Options {
            retry_initial_seconds: 1,
            ..Default::default()
        };
        copy_resume_inner(
            &source,
            &target,
            &o,
            &mut |_, _, _| Ok(()),
            &mut |offset, file| {
                if offset >= 262144 && !injected {
                    injected = true;
                    file.write_all(b"incomplete")?;
                    Err(std::io::ErrorKind::ConnectionReset.into())
                } else {
                    Ok(())
                }
            },
        )
        .unwrap();
        assert!(injected);
        assert_eq!(fs::read(target).unwrap(), bytes);
    }
    #[test]
    fn permanent_failure_is_not_retried() {
        let mut attempts = 0;
        let result = retry_operation(
            &Options::default(),
            &mut || {
                attempts += 1;
                Err::<(), _>(error("permission_denied"))
            },
            &mut || Ok(()),
        );
        assert!(result.is_err());
        assert_eq!(attempts, 1);
    }
}
