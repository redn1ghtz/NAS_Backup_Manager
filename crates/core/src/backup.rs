//! Headless, SQLite-backed backup orchestration. All plans precede mutation.
use crate::model::{validate_relative_path, JobId};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, Failure>;
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub code: String,
    pub detail: String,
}
fn fail(code: &str) -> Failure {
    Failure {
        code: code.into(),
        detail: String::new(),
    }
}
impl From<std::io::Error> for Failure {
    fn from(e: std::io::Error) -> Self {
        Failure {
            code: "filesystem".into(),
            detail: if crate::advanced::transient(&e) {
                format!("transient:{:?}", e.kind())
            } else {
                format!("{:?}", e.kind())
            },
        }
    }
}
impl From<rusqlite::Error> for Failure {
    fn from(_: rusqlite::Error) -> Self {
        fail("database")
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobDraft {
    pub id: Option<String>,
    pub revision: Option<i64>,
    pub name: String,
    pub source: String,
    pub destination: String,
    pub kind: String,
    pub schedule_automatic: bool,
    pub wake_computer: bool,
    pub deletion_guard_enabled: bool,
    pub deletion_guard_min_files: u32,
    pub retention_days: u32,
    pub archive_mode: String,
    pub schedule_kind: String,
    pub schedule_every: u32,
    pub daily_minute: u16,
    pub missed_run: String,
    #[serde(default)]
    pub advanced: crate::advanced::Options,
    #[serde(default, skip_serializing)]
    pub secrets: Option<crate::advanced::Secrets>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedJob {
    pub id: String,
    pub revision: i64,
    pub name: String,
    pub source: String,
    pub destination: String,
    pub kind: String,
    pub schedule_automatic: bool,
    pub wake_computer: bool,
    pub deletion_guard_enabled: bool,
    pub deletion_guard_min_files: u32,
    pub retention_days: u32,
    pub archive_mode: String,
    pub schedule_kind: String,
    pub schedule_every: u32,
    pub daily_minute: u16,
    pub missed_run: String,
    pub enabled: bool,
    pub schedule_enabled: bool,
    pub schedule_pending: bool,
    pub schedule_anchor: String,
    pub current_path: String,
    pub destination_layout: String,
    pub last_result: Option<String>,
    pub last_run: Option<i64>,
    pub last_error: Option<String>,
    pub advanced: crate::advanced::Options,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub id: String,
    pub created: i64,
    pub updated: i64,
    pub deleted: i64,
    pub unchanged: i64,
    pub bytes: i64,
    pub mass_deletion: bool,
    pub database_warning: bool,
    pub archive_disabled: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveEntry {
    pub id: String,
    pub path: String,
    pub archived_at: i64,
    pub expires_at: i64,
    pub bytes: i64,
    pub reason: String,
    pub encrypted: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub job_id: String,
    pub name: String,
    pub state: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub error_code: Option<String>,
    pub trigger_kind: String,
}
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub job_id: String,
    pub run_id: String,
    pub completed: i64,
    pub total: i64,
    pub copied_bytes: i64,
    pub verified_bytes: i64,
    pub eta_seconds: Option<i64>,
    pub total_bytes: i64,
    pub phase: String,
    pub current_path: Option<String>,
}
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
fn id() -> String {
    Uuid::new_v4().to_string()
}
fn text(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| fail("non_unicode_path"))
}
fn identity(path: &Path) -> Result<String> {
    Ok(format!("{:?}", file_id::get_file_id(path)?))
}
fn destination_identity(path: &Path, kind: &str) -> Result<String> {
    #[cfg(windows)]
    {
        let resolved = text(path)?.to_lowercase();
        let is_network = resolved.starts_with("\\\\?\\unc\\")
            || (resolved.starts_with("\\\\") && !resolved.starts_with("\\\\?\\"));
        if (kind == "network") != is_network {
            return Err(fail("destination_type_mismatch"));
        }
        identity(path)
    }
    #[cfg(target_os = "linux")]
    {
        let mounts = fs::read_to_string("/proc/self/mountinfo")?;
        let mut selected: Option<(usize, String)> = None;
        for line in mounts.lines() {
            let Some((left, right)) = line.split_once(" - ") else {
                continue;
            };
            let fields: Vec<_> = left.split_whitespace().collect();
            let details: Vec<_> = right.split_whitespace().collect();
            if fields.len() < 5 || details.len() < 2 {
                continue;
            }
            let mount = fields[4]
                .replace("\\040", " ")
                .replace("\\011", "\t")
                .replace("\\134", "\\");
            if path.starts_with(&mount)
                && selected
                    .as_ref()
                    .is_none_or(|(length, _)| mount.len() > *length)
            {
                let network =
                    ["cifs", "smb3", "nfs", "nfs4", "smbfs", "fuse.sshfs"].contains(&details[0]);
                selected = Some((
                    mount.len(),
                    if network {
                        format!(
                            "{:x}",
                            Sha256::digest(format!("{}:{}", details[0], details[1]))
                        )
                    } else {
                        String::new()
                    },
                ));
            }
        }
        let mount = selected
            .ok_or_else(|| fail("mount_identity_unavailable"))?
            .1;
        if (kind == "network") != !mount.is_empty() {
            return Err(fail("destination_type_mismatch"));
        }
        Ok(if mount.is_empty() {
            identity(path)?
        } else {
            format!("{}:{mount}", identity(path)?)
        })
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        if kind == "network" {
            return Err(fail("mount_identity_unavailable"));
        }
        identity(path)
    }
}
fn overlap(a: &Path, b: &Path) -> bool {
    #[cfg(windows)]
    let (a, b) = (
        PathBuf::from(a.to_string_lossy().to_lowercase()),
        PathBuf::from(b.to_string_lossy().to_lowercase()),
    );
    a.starts_with(&b) || b.starts_with(&a)
}
fn is_link(m: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    m.file_type().is_symlink()
}
fn checked(path: &Path) -> Result<fs::Metadata> {
    for ancestor in path.ancestors() {
        let m = fs::symlink_metadata(ancestor)?;
        if is_link(&m) {
            return Err(fail("link_not_supported"));
        }
    }
    Ok(fs::metadata(path)?)
}
pub(crate) fn directory(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(fail("absolute_path_required"));
    }
    if !checked(path)?.is_dir() {
        return Err(fail("directory_required"));
    }
    let canonical = fs::canonicalize(path)?;
    if canonical.parent().is_none() {
        return Err(fail("protected_directory"));
    }
    for key in [
        "USERPROFILE",
        "HOME",
        "SystemRoot",
        "ProgramFiles",
        "ProgramFiles(x86)",
    ] {
        if let Some(value) = std::env::var_os(key) {
            if let Ok(protected) = fs::canonicalize(value) {
                if canonical == protected
                    || (key != "USERPROFILE" && key != "HOME" && canonical.starts_with(protected))
                {
                    return Err(fail("protected_directory"));
                }
            }
        }
    }
    Ok(canonical)
}
pub(crate) fn root(job: &SavedJob) -> PathBuf {
    let base = Path::new(&job.destination).join(".nas-backup-manager");
    if job.destination_layout == "direct" {
        base
    } else {
        base.join(&job.id)
    }
}
pub(crate) fn current(job: &SavedJob) -> PathBuf {
    if job.destination_layout == "direct" {
        PathBuf::from(&job.destination)
    } else {
        root(job).join("current")
    }
}
pub(crate) fn safe_join(base: &Path, relative: &str) -> Result<PathBuf> {
    validate_relative_path(Path::new(relative)).map_err(|_| fail("unsafe_relative_path"))?;
    let path = base.join(relative);
    // All existing ancestors must be real directories. A missing leaf is allowed.
    for p in path.ancestors() {
        match fs::symlink_metadata(p) {
            Ok(metadata) => {
                // Each ancestor is checked once per call, never cached between
                // operations. Calling checked(p) here rescanned every ancestor
                // of every ancestor (quadratic metadata traffic on deep paths).
                if is_link(&metadata) {
                    return Err(fail("link_not_supported"));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(path)
}

#[cfg(test)]
mod path_checks {
    use super::*;

    #[test]
    fn nested_paths_allow_missing_leaves_but_reject_traversal() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("base");
        fs::create_dir(&base).unwrap();
        assert_eq!(
            safe_join(&base, "new/nested/file").unwrap(),
            base.join("new/nested/file")
        );
        assert_eq!(
            safe_join(&base, "../outside").unwrap_err().code,
            "unsafe_relative_path"
        );
    }

    #[cfg(windows)]
    #[test]
    fn replaced_parent_junction_is_rechecked_even_for_missing_leaf() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("base");
        let outside = temp.path().join("outside");
        let parent = base.join("parent");
        fs::create_dir_all(&parent).unwrap();
        fs::create_dir(&outside).unwrap();
        safe_join(&base, "parent/missing/deep/file").unwrap();
        fs::remove_dir(&parent).unwrap();
        // NTFS junctions need no symlink privilege. All paths belong to this fixture.
        let status = std::process::Command::new("cmd.exe")
            .args(["/C", "mklink", "/J"])
            .arg(&parent)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "{}",
            String::from_utf8_lossy(&status.stderr)
        );
        assert_eq!(
            safe_join(&base, "parent/missing/deep/file")
                .unwrap_err()
                .code,
            "link_not_supported"
        );
        assert_eq!(
            safe_join(&base, "parent").unwrap_err().code,
            "link_not_supported"
        );
        fs::remove_dir(parent).unwrap();
        assert!(outside.is_dir());
    }
}
fn absent(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e.into()),
    }
}
fn lock(job: &SavedJob) -> Result<File> {
    connect_destination(&job.destination, &job.kind)?;
    let path = root(job).join("lock");
    checked(path.parent().unwrap())?;
    if fs::symlink_metadata(&path).is_ok() {
        checked(&path)?;
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(false)
        .truncate(false)
        .open(path)?;
    fs2::FileExt::try_lock_exclusive(&file).map_err(|_| fail("busy"))?;
    Ok(file)
}
fn verify(db: &Connection, job: &SavedJob) -> Result<()> {
    verify_resources(db, job, true)
}
pub(crate) fn verify_resources(
    db: &Connection,
    job: &SavedJob,
    source_required: bool,
) -> Result<()> {
    let (dest, nonce, src, mirror, archive): (String,String,String,String,String) = db.prepare_cached(
        "SELECT j.expected_identity,j.ownership_nonce,r.source_identity,r.current_identity,r.archive_identity FROM jobs j JOIN job_runtime r ON j.id=r.job_id WHERE j.id=?1")?.query_row([&job.id], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
    if source_required {
        directory(Path::new(&job.source))?;
    }
    directory(Path::new(&job.destination))?;
    if (source_required && identity(Path::new(&job.source))? != src)
        || destination_identity(Path::new(&job.destination), &job.kind)? != dest
    {
        return Err(fail("identity_mismatch"));
    }
    for (p, expected) in [(current(job), mirror), (root(job).join("archive"), archive)] {
        if !checked(&p)?.is_dir() || identity(&p)? != expected {
            return Err(fail("identity_mismatch"));
        }
    }
    let marker = root(job).join("owner");
    checked(&marker)?;
    if fs::read_to_string(marker)? != format!("{}\n{}", job.id, nonce) {
        return Err(fail("ownership_mismatch"));
    }
    Ok(())
}
const JOB_SELECT: &str = "SELECT j.id,j.revision,j.name,j.source_path,j.destination_path,j.destination_kind,j.retention_days,j.daily_minute,j.missed_run_policy,j.enabled,j.schedule_enabled,(SELECT state FROM runs WHERE job_id=j.id AND trigger_kind!='preview' ORDER BY started_at DESC,rowid DESC LIMIT 1),(SELECT started_at FROM runs WHERE job_id=j.id AND trigger_kind!='preview' ORDER BY started_at DESC,rowid DESC LIMIT 1),j.archive_mode,CASE WHEN j.schedule_on_login=1 THEN 'startup' ELSE j.schedule_kind END,j.schedule_every,j.schedule_anchor,j.destination_layout,j.deletion_guard_enabled,j.deletion_guard_min_files,j.wake_computer,j.schedule_automatic,j.schedule_pending,(SELECT error_code FROM runs WHERE job_id=j.id AND trigger_kind!='preview' ORDER BY started_at DESC,rowid DESC LIMIT 1),j.advanced_json FROM jobs j";
fn job_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<SavedJob> {
    let mut j = SavedJob {
        id: r.get(0)?,
        revision: r.get(1)?,
        name: r.get(2)?,
        source: r.get(3)?,
        destination: r.get(4)?,
        kind: r.get(5)?,
        retention_days: r.get(6)?,
        daily_minute: r.get(7)?,
        missed_run: r.get(8)?,
        enabled: r.get(9)?,
        schedule_enabled: r.get(10)?,
        current_path: String::new(),
        last_result: r.get(11)?,
        last_run: r.get(12)?,
        last_error: r.get(23)?,
        advanced: serde_json::from_str(&r.get::<_, String>(24)?)
            .map_err(|_| rusqlite::Error::InvalidQuery)?,
        archive_mode: r.get(13)?,
        schedule_kind: r.get(14)?,
        schedule_every: r.get(15)?,
        schedule_anchor: r.get(16)?,
        destination_layout: r.get(17)?,
        deletion_guard_enabled: r.get(18)?,
        deletion_guard_min_files: r.get(19)?,
        wake_computer: r.get(20)?,
        schedule_automatic: r.get(21)?,
        schedule_pending: r.get(22)?,
    };
    j.current_path = current(&j).to_string_lossy().into();
    Ok(j)
}
pub fn get_job(db: &Connection, job_id: &str) -> Result<SavedJob> {
    JobId::parse(job_id).map_err(|_| fail("invalid_job"))?;
    Ok(db.query_row(
        &format!("{JOB_SELECT} WHERE j.id=?1 AND removed_at IS NULL"),
        [job_id],
        job_row,
    )?)
}
fn get_any_job(db: &Connection, job_id: &str) -> Result<SavedJob> {
    JobId::parse(job_id).map_err(|_| fail("invalid_job"))?;
    Ok(db.query_row(&format!("{JOB_SELECT} WHERE j.id=?1"), [job_id], job_row)?)
}
pub fn archive_jobs(db: &Connection, offset: i64) -> Result<Vec<SavedJob>> {
    let mut statement = db.prepare(&format!(
        "{JOB_SELECT} ORDER BY j.created_at,j.id LIMIT 50 OFFSET ?1"
    ))?;
    let rows = statement
        .query_map([offset], job_row)?
        .collect::<std::result::Result<_, _>>()?;
    Ok(rows)
}
pub fn list_jobs(db: &Connection, offset: i64) -> Result<Vec<SavedJob>> {
    recover_incomplete(db)?;
    let mut stmt = db.prepare(&format!(
        "{JOB_SELECT} WHERE j.removed_at IS NULL ORDER BY j.created_at,j.id LIMIT 50 OFFSET ?1"
    ))?;
    let result = Ok(stmt
        .query_map([offset], job_row)?
        .collect::<std::result::Result<_, _>>()?);
    result
}
pub fn valid_schedule(kind: &str, every: u32) -> bool {
    match kind {
        "daily" | "startup" => every == 1,
        "minutes" => (1..=44640).contains(&every),
        "hours" => (1..=744).contains(&every),
        "days" => (1..=365).contains(&every),
        _ => false,
    }
}
pub fn save_job(db: &Connection, mut draft: JobDraft) -> Result<SavedJob> {
    if draft.name.trim().is_empty()
        || draft.retention_days == 0
        || draft.deletion_guard_min_files == 0
        || draft.daily_minute >= 1440
        || !["days", "none"].contains(&draft.archive_mode.as_str())
        || !valid_schedule(&draft.schedule_kind, draft.schedule_every)
        || !["local", "network"].contains(&draft.kind.as_str())
        || !["skip", "run_once_when_available"].contains(&draft.missed_run.as_str())
    {
        return Err(fail("invalid_job"));
    }
    if !draft.schedule_automatic || draft.schedule_kind == "startup" {
        draft.wake_computer = false;
    }
    crate::advanced::validate(&draft.advanced)?;
    crate::advanced::check_remote_isolation(db, draft.id.as_deref(), &draft.advanced)?;
    connect_destination(&draft.destination, &draft.kind)?;
    let source = directory(Path::new(&draft.source))?;
    let destination = directory(Path::new(&draft.destination))?;
    let destination_stamp = destination_identity(&destination, &draft.kind)?;
    if overlap(&source, &destination) {
        return Err(fail("overlap"));
    }
    let transaction = db.unchecked_transaction()?;
    let mut stmt = transaction.prepare("SELECT id,source_path,destination_path FROM jobs")?;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (existing, s, d) = row?;
        if Some(&existing) == draft.id.as_ref() {
            continue;
        }
        if overlap(&destination, Path::new(&d))
            || overlap(&source, Path::new(&d))
            || overlap(&destination, Path::new(&s))
        {
            return Err(fail("overlap"));
        }
    }
    drop(stmt);
    if let Some(ref existing) = draft.id {
        let job = get_job(&transaction, existing)?;
        let _guard = lock(&job)?;
        verify(&transaction, &job)?;
        if draft.revision != Some(job.revision) {
            return Err(fail("stale_job"));
        }
        if source != Path::new(&job.source)
            || destination != Path::new(&job.destination)
            || draft.kind != job.kind
        {
            return Err(fail("paths_immutable"));
        }
        if job.advanced.encryption_recipient.is_empty()
            && !draft.advanced.encryption_recipient.is_empty()
        {
            let legacy: i64 = transaction.query_row(
                "SELECT count(*) FROM archive_versions WHERE job_id=?1 AND state='verified'",
                [&job.id],
                |r| r.get(0),
            )?;
            if legacy > 0 {
                return Err(fail("encryption_legacy_archive"));
            }
        }
        if job.schedule_enabled {
            return Err(fail("disable_schedule_first"));
        }
        transaction.execute("UPDATE jobs SET name=?2,retention_days=?3,daily_minute=?4,missed_run_policy=?5,enabled=0,schedule_enabled=0,approved_revision=NULL,revision=revision+1,updated_at=?6,archive_mode=?7,schedule_kind=CASE WHEN ?8='startup' THEN 'daily' ELSE ?8 END,schedule_on_login=(?8='startup'),schedule_every=?9,schedule_anchor=?10,deletion_guard_enabled=?11,deletion_guard_min_files=?12,wake_computer=?13,schedule_automatic=?14,schedule_pending=?14 WHERE id=?1",params![existing,draft.name.trim(),draft.retention_days,draft.daily_minute,draft.missed_run,now(),draft.archive_mode,draft.schedule_kind,draft.schedule_every,chrono::Local::now().format("%Y-%m-%d").to_string(),draft.deletion_guard_enabled,draft.deletion_guard_min_files,draft.wake_computer,draft.schedule_automatic])?;
        crate::advanced::save_secrets(
            &transaction,
            &mut draft.advanced,
            Some(&job.advanced),
            draft.secrets.as_ref(),
        )?;
        transaction.execute(
            "UPDATE jobs SET advanced_json=?2 WHERE id=?1",
            params![
                existing,
                serde_json::to_string(&draft.advanced).map_err(|_| fail("serialization"))?
            ],
        )?;
        transaction.commit()?;
        return get_job(db, existing);
    }
    if fs::read_dir(&destination)?.next().transpose()?.is_some() {
        return Err(fail("destination_not_empty"));
    }
    crate::advanced::save_secrets(
        &transaction,
        &mut draft.advanced,
        None,
        draft.secrets.as_ref(),
    )?;
    let job_id = id();
    let nonce = id();
    let job = SavedJob {
        id: job_id.clone(),
        revision: 1,
        name: draft.name.trim().into(),
        source: text(&source)?,
        destination: text(&destination)?,
        kind: draft.kind,
        schedule_automatic: draft.schedule_automatic,
        wake_computer: draft.wake_computer,
        deletion_guard_enabled: draft.deletion_guard_enabled,
        deletion_guard_min_files: draft.deletion_guard_min_files,
        retention_days: draft.retention_days,
        archive_mode: draft.archive_mode,
        schedule_kind: draft.schedule_kind,
        schedule_every: draft.schedule_every,
        schedule_anchor: chrono::Local::now().format("%Y-%m-%d").to_string(),
        daily_minute: draft.daily_minute,
        missed_run: draft.missed_run,
        enabled: false,
        schedule_enabled: false,
        schedule_pending: draft.schedule_automatic,
        current_path: String::new(),
        last_result: None,
        last_run: None,
        last_error: None,
        destination_layout: "direct".into(),
        advanced: draft.advanced,
    };
    fs::create_dir(root(&job))?;
    for sub in ["archive", "staging"] {
        fs::create_dir(root(&job).join(sub))?;
    }
    let mut marker = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root(&job).join("owner"))?;
    marker.write_all(format!("{job_id}\n{nonce}").as_bytes())?;
    marker.sync_all()?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root(&job).join("lock"))?
        .sync_all()?;
    transaction.execute("INSERT INTO jobs(id,name,source_path,destination_path,destination_kind,expected_identity,ownership_nonce,retention_days,daily_minute,missed_run_policy,created_at,updated_at,archive_mode,schedule_kind,schedule_every,schedule_anchor,destination_layout,deletion_guard_enabled,deletion_guard_min_files,wake_computer,schedule_automatic,schedule_pending,schedule_on_login) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11,?12,CASE WHEN ?13='startup' THEN 'daily' ELSE ?13 END,?14,?15,?16,?17,?18,?19,?20,?20,(?13='startup'))",params![job.id,job.name,job.source,job.destination,job.kind,destination_stamp,nonce,job.retention_days,job.daily_minute,job.missed_run,now(),job.archive_mode,job.schedule_kind,job.schedule_every,job.schedule_anchor,job.destination_layout,job.deletion_guard_enabled,job.deletion_guard_min_files,job.wake_computer,job.schedule_automatic])?;
    transaction.execute(
        "UPDATE jobs SET advanced_json=?2 WHERE id=?1",
        params![
            job.id,
            serde_json::to_string(&job.advanced).map_err(|_| fail("serialization"))?
        ],
    )?;
    transaction.execute(
        "INSERT INTO job_runtime VALUES(?1,?2,?3,?4)",
        params![
            job.id,
            identity(&source)?,
            identity(&current(&job))?,
            identity(&root(&job).join("archive"))?
        ],
    )?;
    transaction.commit()?;
    get_job(db, &job_id)
}
pub(crate) fn fingerprint_meta(m: &fs::Metadata) -> Result<String> {
    Ok(format!(
        "{:?}",
        m.modified()?
            .duration_since(UNIX_EPOCH)
            .map_err(|_| fail("invalid_timestamp"))?
    ))
}
fn scan(
    db: &Connection,
    run: &str,
    side: i32,
    base: &Path,
    direct: bool,
    cancel: &mut crate::activity::Cancellation,
) -> Result<()> {
    let mut insert_entry =
        db.prepare_cached("INSERT INTO scan_entries VALUES(?1,?2,?3,?4,?5,?6,?7)")?;
    let mut insert_directory =
        db.prepare_cached("INSERT INTO scan_directories(run_id,side,path) VALUES(?1,?2,?3)")?;
    let mut next_directory = db.prepare_cached("SELECT path FROM scan_directories WHERE run_id=?1 AND side=?2 AND done=0 ORDER BY path LIMIT 1")?;
    let mut finish_directory = db.prepare_cached(
        "UPDATE scan_directories SET done=1 WHERE run_id=?1 AND side=?2 AND path=?3",
    )?;
    db.execute(
        "INSERT INTO scan_directories(run_id,side,path) VALUES(?1,?2,'')",
        params![run, side],
    )?;
    loop {
        let next: Option<String> = next_directory
            .query_row(params![run, side], |r| r.get(0))
            .optional()?;
        let Some(relative_directory) = next else {
            break;
        };
        let here = if relative_directory.is_empty() {
            base.to_owned()
        } else {
            safe_join(base, &relative_directory)?
        };
        checked(&here)?;
        for entry in fs::read_dir(&here)? {
            cancel.check()?;
            let entry = entry?;
            if direct
                && relative_directory.is_empty()
                && entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(".nas-backup-manager")
            {
                if side == 1 {
                    continue;
                }
                return Err(fail("reserved_backup_path"));
            }
            let path = entry.path();
            let metadata = checked(&path)?;
            if !metadata.is_file() && !metadata.is_dir() {
                return Err(fail("special_file"));
            }
            let relative = path
                .strip_prefix(base)
                .map_err(|_| fail("unsafe_relative_path"))?;
            validate_relative_path(relative).map_err(|_| fail("unsafe_relative_path"))?;
            let relative = text(relative)?;
            insert_entry.execute(params![
                run,
                side,
                relative,
                metadata.is_dir(),
                i64::try_from(metadata.len()).map_err(|_| fail("file_too_large"))?,
                fingerprint_meta(&metadata)?,
                identity(&path)?
            ])?;
            if metadata.is_dir() {
                insert_directory.execute(params![run, side, relative])?;
            }
        }
        finish_directory.execute(params![run, side, relative_directory])?;
    }
    Ok(())
}
fn create_plan(db: &Connection, job: &SavedJob, trigger: &str) -> Result<Preview> {
    create_plan_checked(
        db,
        job,
        trigger,
        &mut crate::activity::Cancellation::disabled(),
    )
}
fn create_plan_checked(
    db: &Connection,
    job: &SavedJob,
    trigger: &str,
    cancel: &mut crate::activity::Cancellation,
) -> Result<Preview> {
    // Interrupted runs stay visible. They do not silently become successful.
    db.execute("UPDATE runs SET state='interrupted',finished_at=?2,error_code='interrupted' WHERE job_id=?1 AND state IN ('scanning','planned','running')",params![job.id,now()])?;
    let run = id();
    db.execute("INSERT INTO runs(id,job_id,job_revision,trigger_kind,state,started_at) VALUES(?1,?2,?3,?4,'scanning',?5)",params![run,job.id,job.revision,trigger,now()])?;
    let outcome = (|| -> Result<Preview> {
        cancel.check_now()?;
        let tx = db.unchecked_transaction()?;
        scan(
            &tx,
            &run,
            0,
            Path::new(&job.source),
            job.destination_layout == "direct",
            cancel,
        )?;
        scan(
            &tx,
            &run,
            1,
            &current(job),
            job.destination_layout == "direct",
            cancel,
        )?;
        verify(&tx, job)?;
        let conflicts:i64=tx.query_row("SELECT count(*) FROM scan_entries s JOIN scan_entries d ON s.run_id=d.run_id AND s.path=d.path AND d.side=1 WHERE s.run_id=?1 AND s.side=0 AND s.is_dir!=d.is_dir",[&run],|r|r.get(0))?;
        if conflicts > 0 {
            return Err(fail("path_type_conflict"));
        }
        tx.execute("INSERT INTO operations(run_id,sequence,relative_path,kind,state,byte_count) SELECT ?1,row_number() OVER(ORDER BY path),path,kind,'planned',bytes FROM (SELECT s.path,s.bytes,CASE WHEN s.is_dir=1 THEN 'mkdir' WHEN d.path IS NULL THEN 'create' WHEN ((?3='' AND s.bytes=d.bytes AND s.modified=d.modified AND NOT EXISTS(SELECT 1 FROM file_integrity f WHERE f.job_id=?2 AND f.path=s.path AND f.codec!='')) OR EXISTS(SELECT 1 FROM file_integrity f WHERE f.job_id=?2 AND f.path=s.path AND f.source_bytes=s.bytes AND f.source_modified=s.modified AND f.stored_bytes=d.bytes AND f.stored_modified=d.modified AND f.codec=?3)) THEN 'unchanged' ELSE 'update' END kind FROM scan_entries s LEFT JOIN scan_entries d ON d.run_id=s.run_id AND d.path=s.path AND d.side=1 WHERE s.run_id=?1 AND s.side=0 UNION ALL SELECT d.path,d.bytes,'archive_remove' FROM scan_entries d LEFT JOIN scan_entries s ON s.run_id=d.run_id AND s.path=d.path AND s.side=0 WHERE d.run_id=?1 AND d.side=1 AND d.is_dir=0 AND s.path IS NULL)",params![run,job.id,job.advanced.encryption_recipient])?;
        let mut hasher = Sha256::new();
        let mut stmt=tx.prepare("SELECT side,path,is_dir,bytes,modified,identity FROM scan_entries WHERE run_id=?1 ORDER BY side,path")?;
        let mut rows = stmt.query([&run])?;
        while let Some(row) = rows.next()? {
            cancel.check()?;
            let record = (
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            );
            hasher.update(format!("{record:?}"));
        }
        drop(rows);
        drop(stmt);
        tx.execute(
            "INSERT INTO previews(run_id,fingerprint) VALUES(?1,?2)",
            params![run, format!("{:x}", hasher.finalize())],
        )?;
        tx.execute(
            "UPDATE runs SET state='succeeded',scan_complete=1,finished_at=?2 WHERE id=?1",
            params![run, now()],
        )?;
        let count = |kind: &str| -> Result<i64> {
            Ok(tx.query_row(
                "SELECT count(*) FROM operations WHERE run_id=?1 AND kind=?2",
                params![run, kind],
                |r| r.get(0),
            )?)
        };
        let created = count("create")?;
        let updated = count("update")?;
        let deleted = count("archive_remove")?;
        let unchanged = count("unchanged")?;
        let bytes=tx.query_row("SELECT coalesce(sum(byte_count),0) FROM operations WHERE run_id=?1 AND kind IN ('create','update')",[&run],|r|r.get(0))?;
        let database_warning:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM scan_entries WHERE run_id=?1 AND side=0 AND (lower(path) LIKE '%.db' OR lower(path) LIKE '%.sqlite%' OR lower(path) LIKE '%.mdb'))",[&run],|r|r.get(0))?;
        for table in ["scan_entries", "scan_directories", "operations"] {
            tx.execute(&format!("DELETE FROM {table} WHERE run_id IN (SELECT id FROM runs WHERE job_id=?1 AND id!=?2)"),params![job.id,run])?;
        }
        tx.execute("DELETE FROM previews WHERE run_id IN (SELECT id FROM runs WHERE job_id=?1 ORDER BY started_at DESC,rowid DESC LIMIT -1 OFFSET 1000)",[&job.id])?;
        tx.execute("DELETE FROM runs WHERE id IN (SELECT id FROM runs WHERE job_id=?1 ORDER BY started_at DESC,rowid DESC LIMIT -1 OFFSET 1000) AND state IN ('succeeded','failed','interrupted') AND NOT EXISTS(SELECT 1 FROM archive_versions WHERE run_id=runs.id)",[&job.id])?;
        tx.execute("DELETE FROM audit_events WHERE id IN (SELECT id FROM audit_events ORDER BY happened_at DESC,id DESC LIMIT -1 OFFSET 10000)",[])?;
        cancel.check_now()?;
        tx.commit()?;
        Ok(Preview {
            archive_disabled: job.archive_mode == "none",
            id: run.clone(),
            created,
            updated,
            deleted,
            unchanged,
            bytes,
            mass_deletion: job.deletion_guard_enabled
                && deleted >= i64::from(job.deletion_guard_min_files),
            database_warning,
        })
    })();
    if let Err(ref e) = outcome {
        db.execute(
            "UPDATE runs SET state=CASE WHEN ?3='cancelled' THEN 'interrupted' ELSE 'failed' END,finished_at=?2,error_code=?3,trigger_kind=CASE WHEN ?3='cancelled' AND ?4 AND trigger_kind='preview' THEN 'scheduled' ELSE trigger_kind END WHERE id=?1",
            params![run, now(), e.code, cancel.enabled()],
        )?;
    }
    outcome
}
pub fn preview(db: &Connection, job_id: &str) -> Result<Preview> {
    let job = get_job(db, job_id)?;
    let _lock = lock(&job)?;
    verify(db, &job)?;
    create_plan(db, &job, "preview")
}
fn digest(path: &Path) -> Result<String> {
    digest_progress(path, |_| Ok(()))
}
fn digest_progress(path: &Path, mut notify: impl FnMut(i64) -> Result<()>) -> Result<String> {
    let mut read = 0i64;
    let mut last = std::time::Instant::now();
    let mut input = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 262144];
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        read += n as i64;
        if last.elapsed() >= std::time::Duration::from_millis(500) {
            notify(read)?;
            last = std::time::Instant::now();
        }
    }
    notify(read)?;
    Ok(format!("{:x}", hash.finalize()))
}
fn copy_verified(source: &Path, target: &Path) -> Result<String> {
    copy_verified_progress(source, target, |_, _, _| Ok(()))
}
fn copy_verified_progress(
    source: &Path,
    target: &Path,
    mut notify: impl FnMut(i64, i64, &str) -> Result<()>,
) -> Result<String> {
    checked(source)?;
    let before = fs::metadata(source)?;
    let mut input = File::open(source)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    notify(0, 0, "copying")?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 262144];
    let mut copied = 0i64;
    let mut last = std::time::Instant::now();
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        output.write_all(&buf[..n])?;
        hash.update(&buf[..n]);
        copied += n as i64;
        if last.elapsed() >= std::time::Duration::from_millis(500) {
            notify(copied, 0, "copying")?;
            last = std::time::Instant::now();
        }
    }
    filetime::set_file_mtime(
        target,
        filetime::FileTime::from_last_modification_time(&before),
    )?;
    notify(copied, 0, "flushing")?;
    output.sync_all()?;
    drop(output);
    let after = fs::metadata(source)?;
    if before.len() != after.len() || before.modified()? != after.modified()? {
        return Err(fail("file_changed_during_copy"));
    }
    let checksum = format!("{:x}", hash.finalize());
    notify(copied, 0, "verifying")?;
    if digest_progress(target, |verified| notify(copied, verified, "verifying"))? != checksum {
        return Err(fail("integrity_failed"));
    }
    Ok(checksum)
}
fn archive_file(
    db: &Connection,
    job: &SavedJob,
    run: &str,
    relative: &str,
    reason: &str,
    allow_external_edit: bool,
) -> Result<()> {
    verify_resources(db, job, false)?;
    let source = safe_join(&current(job), relative)?;
    let version = id();
    let stored = format!("{version}/payload");
    let archive = root(job).join("archive");
    let folder = safe_join(&archive, &version)?;
    fs::create_dir(&folder)?;
    let archived = now();
    let expires = archived
        .checked_add(i64::from(job.retention_days) * 86400)
        .ok_or_else(|| fail("invalid_retention"))?;
    db.execute("INSERT INTO archive_versions(id,job_id,run_id,original_relative_path,stored_relative_path,reason,archived_at,expires_at,byte_count,state) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'pending')",params![version,job.id,run,relative,stored,reason,archived,expires,i64::try_from(fs::metadata(&source)?.len()).map_err(|_|fail("file_too_large"))?])?;
    let old_codec: Option<String> = db
        .query_row(
            "SELECT codec FROM file_integrity WHERE job_id=?1 AND path=?2",
            params![job.id, relative],
            |r| r.get(0),
        )
        .optional()?;
    let expected_hash: Option<String> = if allow_external_edit {
        None
    } else {
        db.query_row(
            "SELECT sha256 FROM file_integrity WHERE job_id=?1 AND path=?2",
            params![job.id, relative],
            |r| r.get(0),
        )
        .optional()?
    };
    let converting = !job.advanced.encryption_recipient.is_empty()
        && (old_codec.as_deref().unwrap_or("").is_empty() || allow_external_edit);
    if converting {
        if let Some(expected) = &expected_hash {
            if digest(&source)? != *expected {
                return Err(fail("integrity_failed"));
            }
        }
    }
    let copied = if converting {
        crate::advanced::encrypt(
            &source,
            &folder.join("payload"),
            &job.advanced,
            &mut |_, _, _| Ok(()),
        )
    } else {
        crate::advanced::copy_resume(
            &source,
            &folder.join("payload"),
            &job.advanced,
            &mut |_, _, _| Ok(()),
        )
    };
    match copied.and_then(|hash| {
        if !converting && expected_hash.is_some_and(|expected| expected != hash) {
            Err(fail("integrity_failed"))
        } else {
            Ok(hash)
        }
    }) {
        Ok(hash) => {
            db.execute(
                "UPDATE archive_versions SET state='verified',content_hash=?2,codec=?3 WHERE id=?1",
                params![
                    version,
                    hash,
                    if converting {
                        job.advanced.encryption_recipient.as_str()
                    } else {
                        old_codec.as_deref().unwrap_or("")
                    }
                ],
            )?;
            Ok(())
        }
        Err(e) => {
            db.execute(
                "UPDATE archive_versions SET state='failed' WHERE id=?1",
                [version],
            )?;
            Err(e)
        }
    }
}
fn revalidate_entry(
    db: &Connection,
    run: &str,
    side: i32,
    base: &Path,
    relative: &str,
) -> Result<()> {
    let expected:Option<(i64,String,String)>=db.prepare_cached("SELECT bytes,modified,identity FROM scan_entries WHERE run_id=?1 AND side=?2 AND path=?3")?.query_row(params![run,side,relative],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let path = safe_join(base, relative)?;
    match expected {
        None => {
            if !absent(&path)? {
                return Err(fail("stale_preview"));
            }
        }
        Some((bytes, modified, file_id)) => {
            let m = checked(&path)?;
            if i64::try_from(m.len()).ok() != Some(bytes)
                || fingerprint_meta(&m)? != modified
                || identity(&path)? != file_id
            {
                return Err(fail("stale_preview"));
            }
        }
    }
    Ok(())
}
pub fn execute(db: &Connection, job_id: &str, preview_id: &str, consent: bool) -> Result<Preview> {
    execute_progress(db, job_id, preview_id, consent, |_| {})
}
pub fn execute_progress(
    db: &Connection,
    job_id: &str,
    preview_id: &str,
    consent: bool,
    progress: impl FnMut(Progress),
) -> Result<Preview> {
    execute_with_activity(db, job_id, preview_id, consent, progress, false)
}
fn execute_with_activity(
    db: &Connection,
    job_id: &str,
    preview_id: &str,
    consent: bool,
    mut progress: impl FnMut(Progress),
    existing_activity: bool,
) -> Result<Preview> {
    if !consent {
        return Err(fail("consent_required"));
    }
    let job = get_job(db, job_id)?;
    let _activity = if existing_activity {
        None
    } else {
        Some(crate::activity::begin(db, job_id)?)
    };
    let _lock = lock(&job)?;
    let mut cancellation = crate::activity::Cancellation::for_job(db, job_id)?;
    progress(Progress {
        job_id: job_id.into(),
        run_id: String::new(),
        completed: 0,
        total: 0,
        copied_bytes: 0,
        verified_bytes: 0,
        eta_seconds: None,
        total_bytes: 0,
        phase: "scanning".into(),
        current_path: None,
    });
    verify(db, &job)?;
    let expected:Option<(String,i64,i64)>=db.query_row("SELECT p.fingerprint,r.job_revision,r.started_at FROM previews p JOIN runs r ON p.run_id=r.id WHERE p.run_id=?1 AND r.job_id=?2 AND r.state='succeeded' AND r.trigger_kind='preview'",params![preview_id,job_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let (fingerprint, revision, time) = expected.ok_or_else(|| fail("preview_required"))?;
    if revision != job.revision || now() - time > 900 {
        return Err(fail("stale_preview"));
    }
    let plan = create_plan_checked(
        db,
        &job,
        if existing_activity {
            "scheduled"
        } else {
            "manual"
        },
        &mut cancellation,
    )?;
    let actual: String = db.query_row(
        "SELECT fingerprint FROM previews WHERE run_id=?1",
        [&plan.id],
        |r| r.get(0),
    )?;
    if fingerprint != actual {
        db.execute(
            "UPDATE runs SET state='interrupted',error_code='stale_preview' WHERE id=?1",
            [&plan.id],
        )?;
        return Err(fail("stale_preview"));
    }
    db.execute(
        "UPDATE runs SET state='running',finished_at=NULL WHERE id=?1",
        [&plan.id],
    )?;
    let total:i64=db.query_row("SELECT count(*) FROM operations WHERE run_id=?1 AND kind IN ('create','update','archive_remove')",[&plan.id],|r|r.get(0))?;
    let mut completed = 0;
    let mut last_completed_sequence = None;
    let mut last_event = std::time::Instant::now();
    let mut timing = crate::activity::Timing::new();
    let mut snapshot = Progress {
        job_id: job_id.into(),
        run_id: plan.id.clone(),
        completed,
        total,
        copied_bytes: 0,
        verified_bytes: 0,
        eta_seconds: None,
        total_bytes: plan.bytes,
        phase: "copying".into(),
        current_path: None,
    };
    crate::activity::report(db, &snapshot)?;
    progress(snapshot.clone());
    let mut staging_file: Option<PathBuf> = None;
    let mut integrity_pending: Vec<(String, crate::advanced::FileRecord)> = Vec::new();
    let outcome = (|| -> Result<()> {
        cancellation.check_now()?;
        if crate::advanced::verify_due(db, &job)? {
            snapshot.phase = "integrity".into();
            crate::activity::report(db, &snapshot)?;
            progress(snapshot.clone());
            crate::advanced::verify_existing(db, &job, &mut || cancellation.check())?;
        }
        let mut statement = db.prepare(
            "SELECT sequence,relative_path,kind FROM operations WHERE run_id=?1 ORDER BY sequence",
        )?;
        let mut rows = statement.query([&plan.id])?;
        while let Some(row) = rows.next()? {
            cancellation.check()?;
            let sequence: i64 = row.get(0)?;
            let relative: String = row.get(1)?;
            let kind: String = row.get(2)?;
            if kind == "unchanged" {
                continue;
            }
            crate::advanced::retry_operation(&job.advanced, &mut || verify(db, &job), &mut || {
                cancellation.check()
            })?;
            let destination = safe_join(&current(&job), &relative)?;
            if kind == "mkdir" {
                if !destination.exists() {
                    fs::create_dir_all(&destination)?;
                }
                continue;
            }
            snapshot.current_path = Some(relative.clone());
            snapshot.phase = if kind == "archive_remove" {
                "archiving"
            } else {
                "copying"
            }
            .into();
            revalidate_entry(db, &plan.id, 1, &current(&job), &relative)?;
            if kind != "archive_remove" {
                revalidate_entry(db, &plan.id, 0, Path::new(&job.source), &relative)?;
                let stage = root(&job).join("staging");
                checked(&stage)?;
                let temp = stage.join(format!("{}-{}.tmp", plan.id, sequence));
                let source = safe_join(Path::new(&job.source), &relative)?;
                let file_bytes = i64::try_from(fs::metadata(&source)?.len())
                    .map_err(|_| fail("file_too_large"))?;
                let mut report_copy = |bytes, verified, phase: &str| {
                    if staging_file.is_none() {
                        staging_file = Some(temp.clone());
                    }
                    cancellation.check()?;
                    if last_event.elapsed() >= std::time::Duration::from_millis(500)
                        || (file_bytes >= 8 * 1024 * 1024 && snapshot.phase != phase)
                    {
                        // Only allocate strings/sample timing when an update is due.
                        // Cancellation is still checked for every streamed block.
                        let mut update = snapshot.clone();
                        update.copied_bytes = snapshot.copied_bytes + bytes;
                        update.verified_bytes = snapshot.verified_bytes + verified;
                        update.phase = phase.into();
                        update.eta_seconds = timing.estimate(&update);
                        crate::activity::report(db, &update)?;
                        snapshot.phase = phase.into();
                        progress(update);
                        last_event = std::time::Instant::now();
                    }
                    Ok(())
                };
                let checksum = if job.advanced.encryption_recipient.is_empty() {
                    crate::advanced::copy_resume(&source, &temp, &job.advanced, &mut report_copy)?
                } else {
                    crate::advanced::encrypt(&source, &temp, &job.advanced, &mut report_copy)?
                };
                snapshot.copied_bytes += file_bytes;
                snapshot.verified_bytes += file_bytes;
                cancellation.check_now()?;
                if kind != "create" {
                    db.execute(
                        "UPDATE operations SET state='staged' WHERE run_id=?1 AND sequence=?2",
                        params![plan.id, sequence],
                    )?;
                }
                if kind == "update" && job.archive_mode == "days" {
                    archive_file(db, &job, &plan.id, &relative, "replaced", false)?;
                }
                cancellation.check_now()?;
                crate::advanced::retry_operation(
                    &job.advanced,
                    &mut || verify(db, &job),
                    &mut || cancellation.check(),
                )?;
                revalidate_entry(db, &plan.id, 1, &current(&job), &relative)?;
                if let Some(parent) = destination.parent() {
                    checked(parent)?;
                }
                fs::rename(&temp, &destination)?;
                staging_file = None;
                let record = crate::advanced::capture_record(
                    db,
                    &job,
                    &plan.id,
                    &relative,
                    &destination,
                    &checksum,
                )?;
                integrity_pending.push((relative.clone(), record));
                if integrity_pending.len() >= 128
                    || last_event.elapsed() >= std::time::Duration::from_millis(500)
                {
                    crate::advanced::flush_records(db, &job, &mut integrity_pending)?;
                }
            } else {
                if job.archive_mode == "days" {
                    archive_file(db, &job, &plan.id, &relative, "deleted", false)?;
                }
                cancellation.check_now()?;
                crate::advanced::retry_operation(
                    &job.advanced,
                    &mut || verify(db, &job),
                    &mut || cancellation.check(),
                )?;
                revalidate_entry(db, &plan.id, 1, &current(&job), &relative)?;
                // Recheck source absence immediately before mirroring deletion.
                if !absent(&safe_join(Path::new(&job.source), &relative)?)? {
                    return Err(fail("stale_preview"));
                }
                fs::remove_file(&destination)?;
                db.execute(
                    "DELETE FROM file_integrity WHERE job_id=?1 AND path=?2",
                    params![job.id, relative],
                )?;
            }
            if kind != "create" {
                db.execute(
                    "UPDATE operations SET state='committed' WHERE run_id=?1 AND sequence=?2",
                    params![plan.id, sequence],
                )?;
            }
            last_completed_sequence = Some(sequence);
            completed += 1;
            if last_event.elapsed() >= std::time::Duration::from_millis(500) {
                db.execute("UPDATE operations SET state='committed' WHERE run_id=?1 AND kind='create' AND state='planned' AND sequence<=?2",params![plan.id,sequence])?;
                snapshot.completed = completed;
                snapshot.eta_seconds = timing.estimate(&snapshot);
                crate::activity::report(db, &snapshot)?;
                progress(snapshot.clone());
                last_event = std::time::Instant::now();
            }
        }
        cancellation.check_now()?;
        db.execute("UPDATE operations SET state='committed' WHERE run_id=?1 AND state!='committed' AND kind IN ('create','mkdir','unchanged')",[&plan.id])?;
        let mut dirs=db.prepare("SELECT d.path,d.identity FROM scan_entries d LEFT JOIN scan_entries s ON s.run_id=d.run_id AND s.path=d.path AND s.side=0 WHERE d.run_id=?1 AND d.side=1 AND d.is_dir=1 AND s.path IS NULL ORDER BY length(d.path) DESC,d.path")?;
        let mut rows = dirs.query([&plan.id])?;
        while let Some(row) = rows.next()? {
            cancellation.check_now()?;
            crate::advanced::retry_operation(&job.advanced, &mut || verify(db, &job), &mut || {
                cancellation.check()
            })?;
            let relative: String = row.get(0)?;
            let expected: String = row.get(1)?;
            let path = safe_join(&current(&job), &relative)?;
            if identity(&path)? != expected
                || !absent(&safe_join(Path::new(&job.source), &relative)?)?
            {
                return Err(fail("stale_preview"));
            }
            fs::remove_dir(path)?;
        }
        crate::advanced::flush_records(db, &job, &mut integrity_pending)?;
        if job.advanced.cloud_mode != "off" {
            snapshot.phase = "cloud".into();
            snapshot.copied_bytes = 0;
            snapshot.verified_bytes = 0;
            snapshot.total_bytes = 0;
            snapshot.completed = 0;
            snapshot.total = 0;
            snapshot.current_path = None;
            snapshot.eta_seconds = None;
            crate::activity::report(db, &snapshot)?;
            progress(snapshot.clone());
            crate::remote::replicate(
                db,
                &job,
                &mut || cancellation.check(),
                &mut |phase, transfer| {
                    snapshot.phase = phase.into();
                    snapshot.copied_bytes = transfer.bytes;
                    snapshot.total_bytes = transfer.total_bytes;
                    snapshot.completed = transfer.completed;
                    snapshot.total = transfer.total;
                    snapshot.current_path = transfer.current_path;
                    snapshot.eta_seconds = transfer.eta_seconds;
                    crate::activity::report(db, &snapshot)?;
                    progress(snapshot.clone());
                    Ok(())
                },
            )?;
            db.execute("DELETE FROM cloud_capacity WHERE job_id=?1", [&job.id])?;
        }
        Ok(())
    })();
    crate::advanced::flush_records(db, &job, &mut integrity_pending)?;
    if outcome.as_ref().is_err_and(|e| e.code == "cancelled") {
        if let Some(path) = staging_file {
            // Only this run's create_new staging file; never the current mirror or archive.
            if verify_resources(db, &job, false).is_ok() {
                if let Ok(owned) = path.strip_prefix(root(&job)) {
                    if safe_join(&root(&job), &text(owned)?).is_ok_and(|p| p == path)
                        && fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_file())
                    {
                        let _ = fs::remove_file(path);
                    }
                }
            }
        }
    }
    if let Some(sequence) = last_completed_sequence {
        // A later failure must not hide successful copies waiting in the 500 ms batch.
        db.execute("UPDATE operations SET state='committed' WHERE run_id=?1 AND kind='create' AND state='planned' AND sequence<=?2",params![plan.id,sequence])?;
    }
    record_run_metrics(db, &job, &plan.id)?;
    match outcome {
        Ok(()) => {
            db.execute(
                "UPDATE runs SET state='succeeded',finished_at=?2 WHERE id=?1",
                params![plan.id, now()],
            )?;
            db.execute(
                "UPDATE jobs SET approved_revision=revision,enabled=1 WHERE id=?1",
                [job_id],
            )?;
        }
        Err(e) => {
            db.execute(
                "UPDATE runs SET state=CASE WHEN ?3 IN ('stale_preview','file_changed_during_copy','cancelled') THEN 'interrupted' ELSE 'failed' END,finished_at=?2,error_code=?3 WHERE id=?1",
                params![plan.id, now(), e.code],
            )?;
            return Err(e);
        }
    }
    // Expiry is deliberately separate and explicitly requested in the archive UI.
    snapshot.completed = completed;
    snapshot.phase = "finalizing".into();
    snapshot.eta_seconds = None;
    crate::activity::report(db, &snapshot)?;
    progress(snapshot);
    Ok(plan)
}
pub fn verify_integrity(db: &Connection, job_id: &str) -> Result<crate::advanced::IntegrityReport> {
    let job = get_job(db, job_id)?;
    let _activity = crate::activity::begin(db, job_id)?;
    let _lock = lock(&job)?;
    verify_resources(db, &job, false)?;
    let mut cancel = crate::activity::Cancellation::for_job(db, job_id)?;
    crate::advanced::verify_existing(db, &job, &mut || cancel.check())
}
pub fn decrypt_file(db: &Connection, job_id: &str, relative: &str, directory: &Path) -> Result<()> {
    let job = get_job(db, job_id)?;
    let _lock = lock(&job)?;
    verify_resources(db, &job, false)?;
    crate::remote::decrypt_file(db, &job, relative, directory)
}
pub fn decrypt_archive(db: &Connection, version: &str, directory: &Path) -> Result<()> {
    JobId::parse(version).map_err(|_| fail("invalid_path"))?;
    let (job_id,relative,stored,codec):(String,String,String,String)=db.query_row("SELECT job_id,original_relative_path,stored_relative_path,codec FROM archive_versions WHERE id=?1 AND state='verified'",[version],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
    if codec.is_empty() {
        return Err(fail("not_encrypted"));
    }
    let job = get_any_job(db, &job_id)?;
    let _lock = lock(&job)?;
    verify_resources(db, &job, false)?;
    crate::remote::decrypt_source(
        db,
        &job,
        &safe_join(&root(&job).join("archive"), &stored)?,
        &relative,
        directory,
    )
}
pub fn list_archive(db: &Connection, job_id: &str, offset: i64) -> Result<Vec<ArchiveEntry>> {
    get_any_job(db, job_id)?;
    let mut stmt=db.prepare("SELECT id,original_relative_path,archived_at,expires_at,byte_count,reason,(codec!='') FROM archive_versions WHERE job_id=?1 AND state='verified' ORDER BY archived_at DESC,rowid DESC LIMIT 50 OFFSET ?2")?;
    let result = Ok(stmt
        .query_map(params![job_id, offset], |r| {
            Ok(ArchiveEntry {
                id: r.get(0)?,
                path: r.get(1)?,
                archived_at: r.get(2)?,
                expires_at: r.get(3)?,
                bytes: r.get(4)?,
                reason: r.get(5)?,
                encrypted: r.get(6)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?);
    result
}
pub fn restore(db: &Connection, version: &str, _overwrite: bool) -> Result<()> {
    restore_confirmed(db, version, None)
}
fn collision_token(path: &Path) -> Result<String> {
    let m = checked(path)?;
    if !m.is_file() {
        return Err(fail("restore_collision"));
    }
    Ok(format!(
        "{}:{}:{}:{}",
        identity(path)?,
        m.len(),
        fingerprint_meta(&m)?,
        digest(path)?
    ))
}
pub fn restore_confirmed(db: &Connection, version: &str, expected: Option<&str>) -> Result<()> {
    let (job_id,relative,stored,checksum,original_run,codec):(String,String,String,String,String,String)=db.query_row("SELECT job_id,original_relative_path,stored_relative_path,content_hash,run_id,codec FROM archive_versions WHERE id=?1 AND state='verified'",[version],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?;
    let job = get_any_job(db, &job_id)?;
    let _lock = lock(&job)?;
    verify_resources(db, &job, false)?;
    let source = safe_join(&root(&job).join("archive"), &stored)?;
    if digest(&source)? != checksum {
        return Err(fail("integrity_failed"));
    }
    if job.destination_layout == "direct"
        && Path::new(&relative)
            .components()
            .next()
            .is_some_and(|part| {
                part.as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(".nas-backup-manager")
            })
    {
        return Err(fail("reserved_backup_path"));
    }
    let destination = safe_join(&current(&job), &relative)?;
    let collision = if !absent(&destination)? {
        let token = collision_token(&destination)?;
        if expected != Some(token.as_str()) {
            return Err(Failure {
                code: "restore_collision".into(),
                detail: token,
            });
        }
        Some(token)
    } else {
        None
    };
    let stage = root(&job).join("staging");
    checked(&stage)?;
    let temp = stage.join(id());
    copy_verified(&source, &temp)?;
    verify_resources(db, &job, false)?;
    if let Some(token) = collision {
        archive_file(db, &job, &original_run, &relative, "replaced", true)?;
        if collision_token(&destination)? != token {
            return Err(fail("stale_preview"));
        }
        fs::rename(&temp, &destination)?;
    } else {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
            checked(parent)?;
        }
        if fs::hard_link(&temp, &destination).is_err() {
            copy_verified(&temp, &destination)?;
        }
        fs::remove_file(temp)?;
    }
    let metadata = fs::metadata(&destination)?;
    db.execute("INSERT INTO file_integrity(job_id,path,source_bytes,source_modified,stored_bytes,stored_modified,sha256,verified_at,codec) VALUES(?1,?2,0,'restored',?4,?5,?3,?6,?7) ON CONFLICT(job_id,path) DO UPDATE SET sha256=excluded.sha256,source_modified='restored',stored_bytes=excluded.stored_bytes,stored_modified=excluded.stored_modified,verified_at=excluded.verified_at,codec=excluded.codec",params![job.id,relative,checksum,metadata.len()as i64,fingerprint_meta(&metadata)?,now(),codec])?;
    db.execute(
        "INSERT INTO audit_events(id,job_id,kind,happened_at) VALUES(?1,?2,'restore',?3)",
        params![id(), job_id, now()],
    )?;
    Ok(())
}
pub fn cleanup(db: &Connection, job_id: &str, consent: bool) -> Result<u64> {
    let result = cleanup_inner(db, job_id, consent);
    if consent && get_any_job(db, job_id).is_ok() {
        let (state, error) = match &result {
            Ok(_) => ("succeeded", None),
            Err(e) => ("failed", Some(e.code.as_str())),
        };
        db.execute("INSERT INTO audit_events(id,job_id,kind,happened_at,state,error_code) VALUES(?1,?2,'cleanup',?3,?4,?5)",params![id(),job_id,now(),state,error])?;
    }
    result
}
fn cleanup_inner(db: &Connection, job_id: &str, consent: bool) -> Result<u64> {
    if !consent {
        return Err(fail("consent_required"));
    }
    let job = get_any_job(db, job_id)?;
    let _lock = lock(&job)?;
    verify_resources(db, &job, false)?;
    let last:Option<String>=db.query_row("SELECT state FROM runs WHERE job_id=?1 AND trigger_kind!='preview' ORDER BY started_at DESC,rowid DESC LIMIT 1",[job_id],|r|r.get(0)).optional()?;
    if last.as_deref() != Some("succeeded") {
        return Err(fail("successful_run_required"));
    }
    let mut statement=db.prepare("SELECT a.id,a.stored_relative_path,a.content_hash FROM archive_versions a JOIN runs r ON a.run_id=r.id WHERE a.job_id=?1 AND a.expires_at<=?2 AND a.state='verified' AND r.state='succeeded' AND r.scan_complete=1 ORDER BY a.expires_at,a.id")?;
    let mut rows = statement.query(params![job_id, now()])?;
    let mut count = 0;
    while let Some(row) = rows.next()? {
        verify_resources(db, &job, false)?;
        let version: String = row.get(0)?;
        JobId::parse(&version).map_err(|_| fail("ownership_mismatch"))?;
        let stored: String = row.get(1)?;
        if stored != format!("{version}/payload") {
            return Err(fail("ownership_mismatch"));
        }
        let path = safe_join(&root(&job).join("archive"), &stored)?;
        let checksum: String = row.get(2)?;
        if digest(&path)? != checksum {
            return Err(fail("integrity_failed"));
        }
        db.execute(
            "UPDATE archive_versions SET state='expiring' WHERE id=?1",
            [&version],
        )?;
        fs::remove_file(&path)?;
        fs::remove_dir(path.parent().unwrap())?;
        db.execute(
            "UPDATE archive_versions SET state='expired' WHERE id=?1",
            [version],
        )?;
        count += 1;
    }
    Ok(count)
}
pub fn history(db: &Connection, offset: i64) -> Result<Vec<HistoryEntry>> {
    history_filtered(db, offset, false)
}
pub fn history_filtered(
    db: &Connection,
    offset: i64,
    errors_only: bool,
) -> Result<Vec<HistoryEntry>> {
    let mut stmt=db.prepare("SELECT r.id,r.job_id,j.name,r.state,r.started_at,r.finished_at,r.error_code,r.kind FROM (SELECT id,job_id,state,started_at,finished_at,error_code,trigger_kind kind FROM runs UNION ALL SELECT id,job_id,state,happened_at,happened_at,error_code,kind FROM audit_events) r JOIN jobs j ON r.job_id=j.id WHERE (?2=0 OR (r.state IN ('failed','interrupted') AND coalesce(r.error_code,'') NOT IN ('stale_preview','file_changed_during_copy','cancelled'))) ORDER BY r.started_at DESC,r.id DESC LIMIT 50 OFFSET ?1")?;
    let rows = stmt
        .query_map(params![offset, errors_only], |r| {
            Ok(HistoryEntry {
                id: r.get(0)?,
                job_id: r.get(1)?,
                name: r.get(2)?,
                state: r.get(3)?,
                started_at: r.get(4)?,
                finished_at: r.get(5)?,
                error_code: r.get(6)?,
                trigger_kind: r.get(7)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(rows)
}
pub fn pause(db: &Connection, job_id: &str) -> Result<()> {
    let job = get_job(db, job_id)?;
    let _lock = lock(&job)?;
    db.execute(
        "UPDATE jobs SET enabled=0,schedule_enabled=0,schedule_pending=0 WHERE id=?1",
        [job_id],
    )?;
    Ok(())
}
pub fn remove_job(db: &Connection, job_id: &str) -> Result<()> {
    let job = get_job(db, job_id)?;
    let _lock = lock(&job)?;
    if job.schedule_enabled {
        return Err(fail("disable_schedule_first"));
    }
    db.execute(
        "UPDATE jobs SET enabled=0,schedule_enabled=0,schedule_pending=0,removed_at=?2 WHERE id=?1",
        params![job_id, now()],
    )?;
    Ok(())
}
pub fn capacity(db: &Connection, job_id: &str) -> Result<(u64, u64)> {
    let job = get_job(db, job_id)?;
    connect_destination(&job.destination, &job.kind)?;
    verify_resources(db, &job, false)?;
    Ok((
        fs2::total_space(&job.destination)?,
        fs2::available_space(&job.destination)?,
    ))
}

pub fn run_scheduled(db: &Connection, job_id: &str) -> Result<()> {
    run_scheduled_progress(db, job_id, |_| {})
}
pub fn run_scheduled_progress(
    db: &Connection,
    job_id: &str,
    mut progress: impl FnMut(Progress),
) -> Result<()> {
    let job = get_job(db, job_id)?;
    if !job.enabled || !job.schedule_enabled || !job.schedule_automatic {
        return Err(fail("paused"));
    }
    if job.schedule_kind == "days" {
        let last: Option<i64> = db.query_row(
            "SELECT max(started_at) FROM runs WHERE job_id=?1 AND trigger_kind='scheduled'",
            [job_id],
            |r| r.get(0),
        )?;
        if !calendar_due(&job, chrono::Local::now().naive_local(), last)? {
            return Ok(());
        }
    }
    let _activity = crate::activity::begin(db, job_id)?;
    let mut cancellation = crate::activity::Cancellation::for_job(db, job_id)?;
    progress(Progress {
        job_id: job_id.into(),
        run_id: String::new(),
        completed: 0,
        total: 0,
        copied_bytes: 0,
        verified_bytes: 0,
        eta_seconds: None,
        total_bytes: 0,
        phase: "scanning".into(),
        current_path: None,
    });
    let plan = {
        let _lock = lock(&job)?;
        verify(db, &job)?;
        create_plan_checked(db, &job, "preview", &mut cancellation)?
    };
    if plan.mass_deletion {
        let run = id();
        db.execute("INSERT INTO runs(id,job_id,job_revision,trigger_kind,state,started_at,finished_at,error_code) VALUES(?1,?2,?3,'scheduled','failed',?4,?4,'mass_deletion')",params![run,job_id,job.revision,now()])?;
        return Err(fail("mass_deletion"));
    }
    let result = execute_with_activity(db, job_id, &plan.id, true, &mut progress, true)?;
    db.execute(
        "UPDATE runs SET trigger_kind='scheduled' WHERE id=?1",
        [&result.id],
    )?;
    cleanup(db, job_id, true)?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub path: String,
    pub kind: String,
    pub bytes: i64,
}
pub fn operations(db: &Connection, run_id: &str, offset: i64) -> Result<Vec<Operation>> {
    let mut stmt=db.prepare("SELECT relative_path,kind,byte_count FROM operations WHERE run_id=?1 ORDER BY sequence LIMIT 50 OFFSET ?2")?;
    let result = stmt
        .query_map(params![run_id, offset], |r| {
            Ok(Operation {
                path: r.get(0)?,
                kind: r.get(1)?,
                bytes: r.get(2)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(result)
}

/// Reconcile only a job whose OS lock is free and whose destination identity is valid.
/// Old active/archive files are never inferred to be expendable.
pub fn recover_incomplete(db: &Connection) -> Result<()> {
    let mut statement=db.prepare("SELECT id,job_id FROM runs WHERE state IN ('scanning','planned','running') ORDER BY started_at")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let run: String = row.get(0)?;
        let job_id: String = row.get(1)?;
        JobId::parse(&run).map_err(|_| fail("ownership_mismatch"))?;
        let job = get_any_job(db, &job_id)?;
        let Ok(_lock) = lock(&job) else {
            continue;
        };
        if verify_resources(db, &job, false).is_err() {
            continue;
        }
        db.execute("UPDATE runs SET state='interrupted',finished_at=?2,error_code='interrupted' WHERE id=?1",params![run,now()])?;
        let mut operations = db.prepare("SELECT sequence FROM operations WHERE run_id=?1")?;
        let mut items = operations.query([&run])?;
        while let Some(operation) = items.next()? {
            let sequence: i64 = operation.get(0)?;
            let stage = safe_join(
                &root(&job).join("staging"),
                &format!("{run}-{sequence}.tmp"),
            )?;
            if !absent(&stage)? {
                checked(&stage)?;
                fs::remove_file(stage)?;
            }
        }
    }
    Ok(())
}

/// Calendar arithmetic, including across month/year boundaries; no 24-hour DST assumption.
pub fn calendar_due(
    job: &SavedJob,
    local_now: chrono::NaiveDateTime,
    last: Option<i64>,
) -> Result<bool> {
    use chrono::{Days, NaiveDate, NaiveTime};
    if job.schedule_kind != "days" || !valid_schedule(&job.schedule_kind, job.schedule_every) {
        return Err(fail("invalid_job"));
    }
    let anchor = NaiveDate::parse_from_str(&job.schedule_anchor, "%Y-%m-%d")
        .map_err(|_| fail("invalid_job"))?;
    let time = NaiveTime::from_hms_opt(
        u32::from(job.daily_minute / 60),
        u32::from(job.daily_minute % 60),
        0,
    )
    .ok_or_else(|| fail("invalid_job"))?;
    let today = local_now.date();
    let day = if local_now.time() < time {
        today.pred_opt().ok_or_else(|| fail("invalid_job"))?
    } else {
        today
    };
    let elapsed = (day - anchor).num_days();
    if elapsed < 0 {
        return Ok(false);
    }
    let cycle = elapsed / i64::from(job.schedule_every) * i64::from(job.schedule_every);
    let due = anchor
        .checked_add_days(Days::new(cycle as u64))
        .ok_or_else(|| fail("invalid_job"))?
        .and_time(time);
    if job.missed_run == "skip" && due.date() != today {
        return Ok(false);
    }
    let last_local = last
        .and_then(|timestamp| chrono::DateTime::from_timestamp(timestamp, 0))
        .map(|date| date.with_timezone(&chrono::Local).naive_local());
    Ok(last_local.is_none_or(|last| last < due))
}

fn record_run_metrics(db: &Connection, job: &SavedJob, run: &str) -> Result<()> {
    let tx = db.unchecked_transaction()?;
    tx.execute("INSERT OR REPLACE INTO run_metrics SELECT ?1,coalesce(sum(kind='create' AND state='committed'),0),coalesce(sum(kind='update' AND state='committed'),0),coalesce(sum(kind='archive_remove' AND state='committed'),0),coalesce(sum(kind='unchanged'),0),coalesce(sum(CASE WHEN kind='create' AND state='committed' THEN byte_count ELSE 0 END),0),coalesce(sum(CASE WHEN kind='update' AND state='committed' THEN byte_count ELSE 0 END),0),coalesce(sum(CASE WHEN kind='archive_remove' AND state='committed' THEN byte_count ELSE 0 END),0) FROM operations WHERE run_id=?1",[run])?;
    // Keep the most recent 1,000 file outcomes per job, independently of protected archives.
    tx.execute("INSERT INTO file_events(run_id,job_id,relative_path,kind,state,byte_count,happened_at) SELECT ?1,?2,relative_path,CASE WHEN kind='archive_remove' AND ?3='none' THEN 'permanent_remove' ELSE kind END,state,byte_count,?4 FROM (SELECT * FROM operations WHERE run_id=?1 AND kind!='mkdir' ORDER BY sequence DESC LIMIT 1000) ORDER BY sequence",params![run,job.id,job.archive_mode,now()])?;
    tx.execute("DELETE FROM file_events WHERE job_id=?1 AND id NOT IN (SELECT id FROM file_events WHERE job_id=?1 ORDER BY id DESC LIMIT 1000)",[&job.id])?;
    tx.commit()?;
    Ok(())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub state: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub error_code: Option<String>,
    pub created: Option<i64>,
    pub updated: Option<i64>,
    pub deleted: Option<i64>,
    pub unchanged: Option<i64>,
    pub created_bytes: Option<i64>,
    pub updated_bytes: Option<i64>,
    pub deleted_bytes: Option<i64>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEvent {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub kind: String,
    pub state: String,
    pub bytes: i64,
    pub happened_at: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub total_jobs: i64,
    pub successful_jobs: i64,
    pub failed_jobs: i64,
    pub paused_jobs: i64,
    pub archive_files: i64,
    pub archive_bytes: i64,
    pub earliest_expiry: Option<i64>,
    pub last_run: Option<RunSummary>,
    pub recent: Vec<FileEvent>,
}
/// Metadata only: no directory scans, hashes or NAS reads when opening the dashboard.
pub fn dashboard(db: &Connection, selected: Option<&str>) -> Result<Dashboard> {
    if let Some(id) = selected {
        get_any_job(db, id)?;
    }
    let (total_jobs,successful_jobs,failed_jobs,paused_jobs) = db.query_row("SELECT count(*),coalesce(sum(result='succeeded'),0),coalesce(sum(result IN ('failed','interrupted') AND coalesce(last_error,'') NOT IN ('stale_preview','file_changed_during_copy','cancelled')),0),coalesce(sum(enabled=0),0) FROM (SELECT enabled,(SELECT state FROM runs WHERE job_id=j.id AND trigger_kind!='preview' ORDER BY started_at DESC,rowid DESC LIMIT 1) AS result,(SELECT error_code FROM runs WHERE job_id=j.id AND trigger_kind!='preview' ORDER BY started_at DESC,rowid DESC LIMIT 1) AS last_error FROM jobs j WHERE removed_at IS NULL)",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
    let (archive_files,archive_bytes,earliest_expiry) = db.query_row("SELECT count(*),coalesce(sum(byte_count),0),min(expires_at) FROM archive_versions WHERE state='verified' AND (?1 IS NULL OR job_id=?1)",[selected],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    let last_run = db.query_row("SELECT r.state,r.started_at,r.finished_at,r.error_code,m.created_count,m.updated_count,m.deleted_count,m.unchanged_count,m.created_bytes,m.updated_bytes,m.deleted_bytes FROM runs r LEFT JOIN run_metrics m ON m.run_id=r.id WHERE r.trigger_kind!='preview' AND (?1 IS NULL OR r.job_id=?1) ORDER BY r.started_at DESC,r.rowid DESC LIMIT 1",[selected],|r|Ok(RunSummary{state:r.get(0)?,started_at:r.get(1)?,finished_at:r.get(2)?,error_code:r.get(3)?,created:r.get(4)?,updated:r.get(5)?,deleted:r.get(6)?,unchanged:r.get(7)?,created_bytes:r.get(8)?,updated_bytes:r.get(9)?,deleted_bytes:r.get(10)?})).optional()?;
    let mut stmt=db.prepare("SELECT e.id,j.name,e.relative_path,e.kind,e.state,e.byte_count,e.happened_at FROM file_events e JOIN jobs j ON j.id=e.job_id WHERE (?1 IS NULL OR e.job_id=?1) ORDER BY e.id DESC LIMIT 20")?;
    let recent = stmt
        .query_map([selected], |r| {
            Ok(FileEvent {
                id: r.get(0)?,
                name: r.get(1)?,
                path: r.get(2)?,
                kind: r.get(3)?,
                state: r.get(4)?,
                bytes: r.get(5)?,
                happened_at: r.get(6)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(Dashboard {
        total_jobs,
        successful_jobs,
        failed_jobs,
        paused_jobs,
        archive_files,
        archive_bytes,
        earliest_expiry,
        last_run,
        recent,
    })
}

pub fn file_history(db: &Connection, offset: i64, errors_only: bool) -> Result<Vec<FileEvent>> {
    if offset < 0 {
        return Err(fail("invalid_job"));
    }
    let mut stmt=db.prepare("SELECT e.id,j.name,e.relative_path,e.kind,e.state,e.byte_count,e.happened_at FROM file_events e JOIN jobs j ON j.id=e.job_id WHERE (?2=0 OR e.state NOT IN ('committed','unchanged')) ORDER BY e.id DESC LIMIT 50 OFFSET ?1")?;
    let rows = stmt
        .query_map(params![offset, errors_only], |r| {
            Ok(FileEvent {
                id: r.get(0)?,
                name: r.get(1)?,
                path: r.get(2)?,
                kind: r.get(3)?,
                state: r.get(4)?,
                bytes: r.get(5)?,
                happened_at: r.get(6)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(rows)
}

fn connect_destination(path: &str, kind: &str) -> Result<()> {
    if kind == "network" {
        nas_backup_storage::network::ensure_connected(path).map_err(|code| {
            fail(if code == "network_unc_required" {
                "destination_type_mismatch"
            } else {
                code
            })
        })?;
    }
    Ok(())
}
