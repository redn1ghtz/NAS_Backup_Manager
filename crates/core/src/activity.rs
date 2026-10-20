//! Cross-process progress stored locally. File locks prove a worker is still alive.
use crate::{
    backup::{now, Failure, Progress, Result},
    model::JobId,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};
fn error(code: &str) -> Failure {
    Failure {
        code: code.into(),
        detail: String::new(),
    }
}
fn folder(db: &Connection) -> Result<Option<PathBuf>> {
    let name: String = db.query_row(
        "SELECT file FROM pragma_database_list WHERE name='main'",
        [],
        |r| r.get(0),
    )?;
    if name.is_empty() {
        return Ok(None);
    }
    let path = Path::new(&name)
        .parent()
        .ok_or_else(|| error("database"))?
        .join("activity");
    if path.exists() && std::fs::symlink_metadata(&path)?.file_type().is_symlink() {
        return Err(error("ownership_mismatch"));
    }
    Ok(Some(path))
}
pub struct Guard<'a> {
    db: &'a Connection,
    job: String,
    token: String,
    file: Option<File>,
}
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        let _ = self.db.execute(
            "DELETE FROM job_activity WHERE job_id=?1 AND token=?2",
            params![self.job, self.token],
        );
        if let Ok(Some(directory)) = folder(self.db) {
            remove_cancel(&directory, &self.token);
        }
        self.file.take();
    }
}
pub fn begin<'a>(db: &'a Connection, job: &str) -> Result<Guard<'a>> {
    JobId::parse(job).map_err(|_| error("invalid_job"))?;
    let path = if let Some(folder) = folder(db)? {
        std::fs::create_dir_all(&folder)?;
        Some(folder.join(format!("{job}.lock")))
    } else {
        None
    };
    let file = if let Some(path) = &path {
        if path.exists() && std::fs::symlink_metadata(path)?.file_type().is_symlink() {
            return Err(error("ownership_mismatch"));
        }
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        fs2::FileExt::try_lock_exclusive(&f).map_err(|_| error("job_busy"))?;
        Some(f)
    } else {
        None
    };
    if let (Some(directory), Some(previous)) = (
        folder(db)?,
        db.query_row(
            "SELECT token FROM job_activity WHERE job_id=?1",
            [job],
            |r| r.get::<_, String>(0),
        )
        .optional()?,
    ) {
        // The exclusive job activity lock above proves the previous worker ended.
        remove_cancel(&directory, &previous);
    }
    let token = uuid::Uuid::new_v4().to_string();
    db.execute("INSERT INTO job_activity(job_id,token,phase,updated_at) VALUES(?1,?2,'scanning',?3) ON CONFLICT(job_id) DO UPDATE SET token=excluded.token,run_id=NULL,phase='scanning',completed=0,total=0,copied_bytes=0,total_bytes=0,current_path=NULL,verified_bytes=0,eta_seconds=NULL,updated_at=excluded.updated_at",params![job,token,now()])?;
    Ok(Guard {
        db,
        job: job.into(),
        token,
        file,
    })
}
pub fn report(db: &Connection, p: &Progress) -> Result<()> {
    db.execute("UPDATE job_activity SET run_id=?2,phase=?3,completed=?4,total=?5,copied_bytes=?6,total_bytes=?7,current_path=?8,updated_at=?9,verified_bytes=?10,eta_seconds=?11 WHERE job_id=?1",params![p.job_id,p.run_id,p.phase,p.completed,p.total,p.copied_bytes,p.total_bytes,p.current_path,now(),p.verified_bytes,p.eta_seconds])?;
    Ok(())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveProgress {
    pub execution_id: String,
    pub cancel_requested: bool,
    pub job_id: String,
    pub name: String,
    pub phase: String,
    pub completed: i64,
    pub total: i64,
    pub copied_bytes: i64,
    pub verified_bytes: i64,
    pub eta_seconds: Option<i64>,
    pub total_bytes: i64,
    pub current_path: Option<String>,
    pub updated_at: i64,
}
pub fn list(db: &Connection) -> Result<Vec<LiveProgress>> {
    let mut statement=db.prepare("SELECT a.job_id,j.name,a.phase,a.completed,a.total,a.copied_bytes,a.total_bytes,a.current_path,a.updated_at,a.token,a.verified_bytes,a.eta_seconds FROM job_activity a JOIN jobs j ON j.id=a.job_id ORDER BY a.updated_at DESC LIMIT 50")?;
    let mut items = statement
        .query_map([], |r| {
            Ok(LiveProgress {
                job_id: r.get(0)?,
                name: r.get(1)?,
                phase: r.get(2)?,
                completed: r.get(3)?,
                total: r.get(4)?,
                copied_bytes: r.get(5)?,
                total_bytes: r.get(6)?,
                current_path: r.get(7)?,
                updated_at: r.get(8)?,
                execution_id: r.get(9)?,
                cancel_requested: false,
                verified_bytes: r.get(10)?,
                eta_seconds: r.get(11)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if let Some(folder) = folder(db)? {
        items.retain(|item| {
            if JobId::parse(&item.job_id).is_err() {
                return false;
            }
            let path = folder.join(format!("{}.lock", item.job_id));
            let alive = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .is_ok_and(|f| fs2::FileExt::try_lock_exclusive(&f).is_err());
            if !alive {
                remove_cancel(&folder, &item.execution_id);
                let _ = db.execute(
                    "DELETE FROM job_activity WHERE job_id=?1 AND token=?2",
                    params![item.job_id, item.execution_id],
                );
            }
            alive
        });
        for item in &mut items {
            item.cancel_requested =
                cancellation_path(&folder, &item.execution_id).is_ok_and(|path| path.is_file());
        }
    }
    Ok(items)
}

fn cancellation_path(directory: &Path, token: &str) -> Result<PathBuf> {
    if !uuid::Uuid::parse_str(token).is_ok_and(|id| id.to_string() == token) {
        return Err(error("invalid_job"));
    }
    Ok(directory.join(format!("{token}.cancel")))
}
fn remove_cancel(directory: &Path, token: &str) {
    if let Ok(path) = cancellation_path(directory, token) {
        if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_file()) {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Local token-bound marker works even while a scan holds a SQLite transaction.
/// Never kills a process or writes to a backup destination.
pub fn request_cancel(db: &Connection, job: &str, token: &str) -> Result<bool> {
    JobId::parse(job).map_err(|_| error("invalid_job"))?;
    let Some(directory) = folder(db)? else {
        return Ok(false);
    };
    let path = cancellation_path(&directory, token)?;
    let current = || -> Result<bool> {
        Ok(db.query_row("SELECT 1 FROM job_activity WHERE job_id=?1 AND token=?2 AND phase!='finalizing' AND (run_id IS NULL OR run_id='' OR EXISTS(SELECT 1 FROM runs WHERE id=run_id AND state IN ('scanning','planned','running')))",params![job,token], |_| Ok(true)).optional()?.unwrap_or(false))
    };
    if !current()? {
        return Ok(false);
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(directory.join(format!("{job}.lock")))?;
    if fs2::FileExt::try_lock_exclusive(&lock).is_ok() {
        return Ok(false);
    }
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(file) => drop(file),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if !std::fs::symlink_metadata(&path)?.file_type().is_file() {
                return Err(error("ownership_mismatch"));
            }
        }
        Err(e) => return Err(e.into()),
    }
    if !current()? {
        remove_cancel(&directory, token);
        return Ok(false);
    }
    Ok(true)
}

pub(crate) struct Cancellation {
    path: Option<PathBuf>,
    checked_at: std::time::Instant,
}
impl Cancellation {
    pub(crate) fn disabled() -> Self {
        Self {
            path: None,
            checked_at: std::time::Instant::now(),
        }
    }
    pub(crate) fn for_job(db: &Connection, job: &str) -> Result<Self> {
        let token: Option<String> = db
            .query_row(
                "SELECT token FROM job_activity WHERE job_id=?1",
                [job],
                |r| r.get(0),
            )
            .optional()?;
        let path = match (folder(db)?, token) {
            (Some(directory), Some(token)) => Some(cancellation_path(&directory, &token)?),
            _ => None,
        };
        Ok(Self {
            path,
            checked_at: std::time::Instant::now() - std::time::Duration::from_secs(1),
        })
    }
    pub(crate) fn enabled(&self) -> bool {
        self.path.is_some()
    }
    pub(crate) fn check(&mut self) -> Result<()> {
        if self.checked_at.elapsed() >= std::time::Duration::from_millis(250) {
            self.check_now()?;
        }
        Ok(())
    }
    pub(crate) fn check_now(&mut self) -> Result<()> {
        self.checked_at = std::time::Instant::now();
        if let Some(path) = &self.path {
            match std::fs::symlink_metadata(path) {
                Ok(metadata) if metadata.file_type().is_file() => return Err(error("cancelled")),
                Ok(_) => return Err(error("ownership_mismatch")),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}

/// Native presentation is best-effort and cannot change backup success or safety.
pub struct Feedback<'a> {
    db: &'a Connection,
    job: String,
    name: String,
    state: PathBuf,
    host: Option<nas_backup_storage::desktop_activity::DesktopActivity>,
    attempted: bool,
    failed: bool,
}
impl<'a> Feedback<'a> {
    pub fn new(db: &'a Connection, job: &crate::backup::SavedJob, state: &Path) -> Self {
        Self {
            db,
            job: job.id.clone(),
            name: job.name.clone(),
            state: state.into(),
            host: None,
            attempted: false,
            failed: false,
        }
    }
    fn start(&mut self) {
        if self.attempted || !cfg!(windows) {
            return;
        }
        self.attempted = true;
        let settings: rusqlite::Result<(String, bool)> = self.db.query_row(
            "SELECT language,notifications_enabled FROM settings WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        );
        if let Ok((language, enabled)) = settings {
            match nas_backup_storage::desktop_activity::DesktopActivity::start(
                &self.state,
                &self.name,
                &language,
                enabled,
            ) {
                Ok(host) => self.host = Some(host),
                Err(_) => self.failed = true,
            }
        } else {
            self.failed = true;
        }
    }
    pub fn progress(&mut self, p: &Progress) {
        self.start();
        if let Some(host) = self.host.as_mut() {
            host.progress(
                if p.total_bytes > 0 {
                    p.copied_bytes.saturating_add(p.verified_bytes)
                } else {
                    p.completed
                },
                if p.total_bytes > 0 {
                    p.total_bytes.saturating_mul(2)
                } else {
                    p.total
                },
            );
        }
    }
    pub fn finish(mut self, error: Option<&Failure>) {
        if let Some(error) = error {
            if !["paused", "job_busy", "consent_required"].contains(&error.code.as_str()) {
                self.start();
            }
        }
        let outcome = match error {
            None => "succeeded",
            Some(e) if ["stale_preview", "file_changed_during_copy"].contains(&e.code.as_str()) => {
                "changed"
            }
            Some(e) if e.code == "cancelled" => "cancelled",
            Some(_) => "failed",
        };
        if let Some(mut host) = self.host.take() {
            if let Ok(enabled) = self.db.query_row(
                "SELECT notifications_enabled FROM settings WHERE singleton=1",
                [],
                |r| r.get::<_, bool>(0),
            ) {
                host.set_notifications(enabled);
            }
            self.failed |= host.finish(outcome).is_err();
        }
        if self.failed {
            let _=self.db.execute("INSERT INTO audit_events(id,job_id,kind,happened_at,state,error_code) VALUES(?1,?2,'notification',?3,'failed','notification_unavailable')",params![uuid::Uuid::new_v4().to_string(),self.job,now()]);
        }
    }
}

/// Bounded recent throughput across copying/readback, including batches of small files.
pub struct Timing {
    samples: std::collections::VecDeque<(std::time::Instant, i64)>,
}
impl Default for Timing {
    fn default() -> Self {
        Self::new()
    }
}
impl Timing {
    pub fn new() -> Self {
        Self {
            samples: std::collections::VecDeque::from([(std::time::Instant::now(), 0)]),
        }
    }
    pub fn estimate(&mut self, p: &Progress) -> Option<i64> {
        self.observe(
            std::time::Instant::now(),
            p.copied_bytes.saturating_add(p.verified_bytes),
            p.total_bytes.saturating_mul(2),
            &p.phase,
        )
    }
    fn observe(
        &mut self,
        now: std::time::Instant,
        done: i64,
        total: i64,
        phase: &str,
    ) -> Option<i64> {
        if self.samples.back().is_none_or(|(time, _)| {
            now.duration_since(*time) >= std::time::Duration::from_millis(500)
        }) {
            self.samples.push_back((now, done));
        }
        while self.samples.len() > 1
            && self.samples.front().is_some_and(|(time, _)| {
                now.duration_since(*time) > std::time::Duration::from_secs(10)
            })
        {
            self.samples.pop_front();
        }
        while self.samples.len() > 21 {
            self.samples.pop_front();
        }
        if !["copying", "verifying"].contains(&phase) || total <= 0 {
            return None;
        }
        let (time, baseline) = self.samples.front()?;
        remaining_seconds(
            done.saturating_sub(*baseline),
            total.saturating_sub(done),
            now.duration_since(*time).as_secs_f64(),
        )
    }
}
fn remaining_seconds(advanced: i64, remaining: i64, elapsed: f64) -> Option<i64> {
    if advanced <= 0 || remaining <= 0 || elapsed < 1.0 {
        return None;
    }
    Some(
        (remaining as f64 * elapsed / advanced as f64)
            .ceil()
            .max(1.) as i64,
    )
}
#[cfg(test)]
mod timing_tests {
    #[test]
    fn eta_handles_small_file_phases_and_discards_stalled_samples() {
        let start = std::time::Instant::now();
        let mut meter = super::Timing {
            samples: std::collections::VecDeque::from([(start, 0)]),
        };
        assert_eq!(
            meter.observe(
                start + std::time::Duration::from_secs(1),
                100,
                400,
                "copying"
            ),
            Some(3)
        );
        assert_eq!(
            meter.observe(
                start + std::time::Duration::from_millis(1500),
                200,
                400,
                "verifying"
            ),
            Some(2)
        );
        assert_eq!(
            meter.observe(
                start + std::time::Duration::from_secs(2),
                200,
                400,
                "flushing"
            ),
            None
        );
        assert_eq!(
            meter.observe(
                start + std::time::Duration::from_secs(20),
                200,
                400,
                "verifying"
            ),
            None
        );
        assert_eq!(
            meter.observe(
                start + std::time::Duration::from_secs(21),
                300,
                400,
                "verifying"
            ),
            Some(1)
        );
    }
    #[test]
    fn eta_requires_measured_progress_and_never_claims_completion() {
        assert_eq!(super::remaining_seconds(50, 150, 2.), Some(6));
        assert_eq!(super::remaining_seconds(0, 150, 2.), None);
        assert_eq!(super::remaining_seconds(50, 0, 2.), None);
        assert_eq!(super::remaining_seconds(50, 150, 0.5), None);
    }
}
