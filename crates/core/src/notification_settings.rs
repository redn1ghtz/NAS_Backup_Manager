//! Shared report destinations. SQLite stores metadata + an immutable DPAPI blob ID only.
use crate::{
    advanced::{self, Options, Secrets},
    backup::{self, Result},
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Fields {
    pub telegram_chat: String,
    pub smtp_url: String,
    pub smtp_username: String,
    pub smtp_from: String,
    pub smtp_to: String,
    pub curl_executable: String,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Profile {
    fields: Fields,
    vault_id: String,
    telegram: bool,
    smtp: bool,
}
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Update {
    pub fields: Fields,
    pub telegram_token: String,
    pub smtp_password: String,
}
impl std::fmt::Debug for Update {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotificationUpdate([REDACTED])")
    }
}
#[derive(Serialize)]
pub struct Legacy {
    id: String,
    name: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub fields: Fields,
    pub telegram_configured: bool,
    pub smtp_configured: bool,
    pub legacy_jobs: Vec<Legacy>,
    pub legacy_more: bool,
}
fn read(db: &Connection) -> Result<Profile> {
    let json: String = db.query_row(
        "SELECT profile_json FROM notification_settings WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    if json.len() > 65536 {
        return Err(advanced::error("invalid_advanced"));
    }
    serde_json::from_str(&json).map_err(|_| advanced::error("invalid_advanced"))
}
fn secret(db: &Connection, profile: &Profile) -> Result<Secrets> {
    if profile.vault_id.is_empty() {
        return Ok(Secrets::default());
    }
    let bytes = nas_backup_storage::vault::load(&advanced::vault_path(db, &profile.vault_id)?)
        .map_err(advanced::error)?;
    serde_json::from_slice(&bytes).map_err(|_| advanced::error("vault_unavailable"))
}
fn default_curl() -> String {
    #[cfg(windows)]
    {
        std::env::var_os("SystemRoot")
            .map(|p| {
                std::path::PathBuf::from(p)
                    .join("System32/curl.exe")
                    .to_string_lossy()
                    .into_owned()
            })
            .unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        "/usr/bin/curl".into()
    }
}
pub fn get(db: &Connection, offset: i64) -> Result<View> {
    if offset < 0 {
        return Err(advanced::error("invalid_advanced"));
    }
    let p = read(db)?;
    let mut fields = p.fields;
    if fields.curl_executable.is_empty() {
        fields.curl_executable = default_curl();
    }
    let mut q=db.prepare("SELECT id,name FROM jobs WHERE removed_at IS NULL AND ((json_extract(advanced_json,'$.telegramEnabled')=1 AND coalesce(json_extract(advanced_json,'$.telegramChat'),'')!='') OR (json_extract(advanced_json,'$.smtpEnabled')=1 AND coalesce(json_extract(advanced_json,'$.smtpTo'),'')!='')) ORDER BY name,id LIMIT 51 OFFSET ?1")?;
    let mut legacy_jobs = q
        .query_map([offset], |r| {
            Ok(Legacy {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let legacy_more = legacy_jobs.len() > 50;
    legacy_jobs.truncate(50);
    Ok(View {
        fields,
        telegram_configured: p.telegram,
        smtp_configured: p.smtp,
        legacy_jobs,
        legacy_more,
    })
}
fn apply(profile: &Profile, global: &Secrets, o: &mut Options, s: &mut Secrets) {
    if profile.telegram {
        o.telegram_chat = profile.fields.telegram_chat.clone();
        s.telegram_token = global.telegram_token.clone();
    }
    if profile.smtp {
        o.smtp_url = profile.fields.smtp_url.clone();
        o.smtp_username = profile.fields.smtp_username.clone();
        o.smtp_from = profile.fields.smtp_from.clone();
        o.smtp_to = profile.fields.smtp_to.clone();
        s.smtp_password = global.smtp_password.clone();
    }
    if (o.telegram_enabled && profile.telegram) || (o.smtp_enabled && profile.smtp) {
        o.curl_executable = profile.fields.curl_executable.clone();
    }
}
fn validate(o: &Options, s: &Secrets) -> Result<()> {
    advanced::validate_notification_options(o)?;
    if o.telegram_enabled
        && (s.telegram_token.is_empty()
            || !s.telegram_token.contains(':')
            || !s
                .telegram_token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b)))
        || o.smtp_enabled && s.smtp_password.is_empty()
    {
        return Err(advanced::error("notification_settings_required"));
    }
    if s.telegram_token.len() > 16384 || s.smtp_password.len() > 16384 {
        return Err(advanced::error("invalid_advanced"));
    }
    Ok(())
}
pub fn validate_job(db: &Connection, o: &Options, s: &Secrets) -> Result<()> {
    let p = read(db)?;
    let mut effective = o.clone();
    let mut values = s.clone();
    if (o.telegram_enabled && p.telegram) || (o.smtp_enabled && p.smtp) {
        apply(&p, &secret(db, &p)?, &mut effective, &mut values);
    }
    if o.telegram_enabled && effective.telegram_chat.is_empty()
        || o.smtp_enabled && effective.smtp_to.is_empty()
    {
        return Err(advanced::error("notification_settings_required"));
    }
    validate(&effective, &values)
}
pub fn effective(db: &Connection, o: &Options) -> Result<(Options, Secrets)> {
    let p = read(db)?;
    let mut effective = o.clone();
    // Read a job vault only for legacy channels; cloud/age credentials are unrelated.
    let mut values = if o.telegram_enabled && !p.telegram || o.smtp_enabled && !p.smtp {
        let mut legacy = o.clone();
        legacy.cloud_mode = "off".into();
        advanced::secrets(db, &legacy)?
    } else {
        Secrets::default()
    };
    if (o.telegram_enabled && p.telegram) || (o.smtp_enabled && p.smtp) {
        apply(&p, &secret(db, &p)?, &mut effective, &mut values);
    }
    validate(&effective, &values)?;
    Ok((effective, values))
}
fn persist(db: &Connection, fields: Fields, values: Secrets) -> Result<View> {
    let mut fields = fields;
    if [
        &fields.telegram_chat,
        &fields.smtp_url,
        &fields.smtp_username,
        &fields.smtp_from,
        &fields.smtp_to,
        &fields.curl_executable,
    ]
    .iter()
    .any(|v| v.len() > 4096)
    {
        return Err(advanced::error("invalid_advanced"));
    }
    if fields.curl_executable.is_empty() {
        fields.curl_executable = default_curl();
    }
    let telegram = !fields.telegram_chat.is_empty() || !values.telegram_token.is_empty();
    let smtp = !fields.smtp_from.is_empty()
        || !fields.smtp_to.is_empty()
        || !values.smtp_password.is_empty();
    let o = Options {
        telegram_enabled: telegram,
        telegram_chat: fields.telegram_chat.clone(),
        smtp_enabled: smtp,
        smtp_url: fields.smtp_url.clone(),
        smtp_username: fields.smtp_username.clone(),
        smtp_from: fields.smtp_from.clone(),
        smtp_to: fields.smtp_to.clone(),
        curl_executable: fields.curl_executable.clone(),
        ..Default::default()
    };
    validate(&o, &values)?;
    let vault_id = uuid::Uuid::new_v4().to_string();
    // No transaction is held during DPAPI. Existing encrypted blobs stay valid for readers.
    nas_backup_storage::vault::save_new(
        &advanced::vault_path(db, &vault_id)?,
        &serde_json::to_vec(&values).map_err(|_| advanced::error("vault_unavailable"))?,
    )
    .map_err(advanced::error)?;
    let profile = Profile {
        fields,
        vault_id,
        telegram,
        smtp,
    };
    db.execute(
        "UPDATE notification_settings SET profile_json=?1 WHERE singleton=1",
        [serde_json::to_string(&profile).map_err(|_| advanced::error("invalid_advanced"))?],
    )?;
    get(db, 0)
}
fn lock(db: &Connection) -> Result<std::fs::File> {
    let path = advanced::state_directory(db)?.join("notification-settings.lock");
    if std::fs::symlink_metadata(&path).is_ok_and(|m| !m.file_type().is_file()) {
        return Err(advanced::error("vault_unavailable"));
    }
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)?;
    fs2::FileExt::try_lock_exclusive(&f).map_err(|_| advanced::error("job_busy"))?;
    Ok(f)
}
pub fn save(db: &Connection, update: Update) -> Result<View> {
    let _lock = lock(db)?;
    let p = read(db)?;
    let mut values = match secret(db, &p) {
        Ok(values) => values,
        // A moved/corrupt vault can be replaced only when every configured
        // channel's secret is explicitly supplied; never drop another channel.
        Err(_)
            if (!p.telegram || !update.telegram_token.is_empty())
                && (!p.smtp || !update.smtp_password.is_empty()) =>
        {
            Secrets::default()
        }
        Err(e) => return Err(e),
    };
    if !update.telegram_token.is_empty() {
        values.telegram_token = update.telegram_token;
    }
    if !update.smtp_password.is_empty() {
        values.smtp_password = update.smtp_password;
    }
    persist(db, update.fields, values)
}
pub fn import_job(db: &Connection, id: &str) -> Result<View> {
    let _lock = lock(db)?;
    let job = backup::get_job(db, id)?;
    let mut o = job.advanced;
    o.cloud_mode = "off".into();
    let old = advanced::secrets(db, &o)?;
    let p = read(db)?;
    let mut fields = p.fields.clone();
    let mut values = secret(db, &p)?;
    advanced::validate_notification_options(&o)?;
    if o.telegram_enabled {
        fields.telegram_chat = o.telegram_chat;
        values.telegram_token = old.telegram_token;
    }
    if o.smtp_enabled {
        fields.smtp_url = o.smtp_url;
        fields.smtp_username = o.smtp_username;
        fields.smtp_from = o.smtp_from;
        fields.smtp_to = o.smtp_to;
        values.smtp_password = old.smtp_password;
    }
    if fields.curl_executable.is_empty() {
        fields.curl_executable = o.curl_executable;
    }
    persist(db, fields, values)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, nas_backup_storage::database::Database) {
        let t = tempfile::tempdir().unwrap();
        let db =
            nas_backup_storage::database::Database::open(&t.path().join("state.sqlite")).unwrap();
        (t, db)
    }
    fn fields() -> Fields {
        Fields {
            telegram_chat: "12345".into(),
            smtp_url: "smtps://smtp.example.invalid:465".into(),
            smtp_username: "fixture@example.invalid".into(),
            smtp_from: "fixture@example.invalid".into(),
            smtp_to: "recipient@example.invalid".into(),
            curl_executable: std::env::current_exe().unwrap().to_string_lossy().into(),
        }
    }
    #[test]
    fn global_secrets_are_protected_retained_and_never_returned() {
        let (t, db) = fixture();
        let token = "123:dummy_fixture_token";
        let pass = "fixture-only-smtp-password";
        let view = save(
            &db.connection,
            Update {
                fields: fields(),
                telegram_token: token.into(),
                smtp_password: pass.into(),
            },
        )
        .unwrap();
        assert!(view.telegram_configured && view.smtp_configured);
        let json = serde_json::to_string(&view).unwrap();
        assert!(!json.contains(token) && !json.contains(pass));
        let stored: String = db
            .connection
            .query_row("SELECT profile_json FROM notification_settings", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(!stored.contains(token) && !stored.contains(pass));
        let profile = read(&db.connection).unwrap();
        let bytes = std::fs::read(advanced::vault_path(&db.connection, &profile.vault_id).unwrap())
            .unwrap();
        assert!(!bytes.windows(token.len()).any(|b| b == token.as_bytes()));
        let mut changed = view.fields;
        changed.smtp_to = "updated@example.invalid".into();
        save(
            &db.connection,
            Update {
                fields: changed,
                ..Default::default()
            },
        )
        .unwrap();
        let flags = Options {
            telegram_enabled: true,
            smtp_enabled: true,
            ..Default::default()
        };
        validate_job(&db.connection, &flags, &Secrets::default()).unwrap();
        let (options, values) = effective(&db.connection, &flags).unwrap();
        assert_eq!(options.smtp_to, "updated@example.invalid");
        assert_eq!(values.telegram_token, token);
        assert_eq!(values.smtp_password, pass);
        assert_eq!(get(&db.connection, 0).unwrap().legacy_jobs.len(), 0);
        assert!(t.path().join("state.sqlite").exists());
    }
    #[test]
    fn missing_and_invalid_settings_do_not_replace_a_valid_profile() {
        let (_t, db) = fixture();
        assert_eq!(
            validate_job(
                &db.connection,
                &Options {
                    telegram_enabled: true,
                    ..Default::default()
                },
                &Secrets::default()
            )
            .unwrap_err()
            .code,
            "notification_settings_required"
        );
        save(
            &db.connection,
            Update {
                fields: fields(),
                telegram_token: "1:fixture_token".into(),
                smtp_password: "fixture_password".into(),
            },
        )
        .unwrap();
        let before: String = db
            .connection
            .query_row("SELECT profile_json FROM notification_settings", [], |r| {
                r.get(0)
            })
            .unwrap();
        let mut unsafe_fields = fields();
        unsafe_fields.smtp_url = "http://untrusted.invalid".into();
        assert!(save(
            &db.connection,
            Update {
                fields: unsafe_fields,
                ..Default::default()
            }
        )
        .is_err());
        let mut unsafe_fields = fields();
        unsafe_fields.smtp_to = "a@b\r\nBcc:x@y".into();
        assert!(save(
            &db.connection,
            Update {
                fields: unsafe_fields,
                ..Default::default()
            }
        )
        .is_err());
        assert_eq!(
            before,
            db.connection
                .query_row("SELECT profile_json FROM notification_settings", [], |r| {
                    r.get::<_, String>(0)
                })
                .unwrap()
        );
    }
    #[test]
    fn inaccessible_vault_requires_all_existing_secrets_before_replacement() {
        let (_t, db) = fixture();
        save(
            &db.connection,
            Update {
                fields: fields(),
                telegram_token: "1:fixture".into(),
                smtp_password: "fixture_password".into(),
            },
        )
        .unwrap();
        let p = read(&db.connection).unwrap();
        let old = p.vault_id.clone();
        std::fs::write(
            advanced::vault_path(&db.connection, &old).unwrap(),
            b"invalid disposable protected blob",
        )
        .unwrap();
        assert!(save(
            &db.connection,
            Update {
                fields: fields(),
                telegram_token: "2:new_fixture".into(),
                ..Default::default()
            }
        )
        .is_err());
        assert_eq!(read(&db.connection).unwrap().vault_id, old);
        save(
            &db.connection,
            Update {
                fields: fields(),
                telegram_token: "2:new_fixture".into(),
                smtp_password: "new_fixture_password".into(),
            },
        )
        .unwrap();
        let (_, secrets) = effective(
            &db.connection,
            &Options {
                telegram_enabled: true,
                smtp_enabled: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(secrets.telegram_token, "2:new_fixture");
        assert_eq!(secrets.smtp_password, "new_fixture_password");
    }

    #[test]
    fn legacy_import_is_explicit_preserves_jobs_and_merges_channels() {
        let (t, db) = fixture();
        let source = t.path().join("source");
        let dest = t.path().join("dest");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&dest).unwrap();
        let mut old = Options {
            telegram_enabled: true,
            telegram_chat: "777".into(),
            curl_executable: fields().curl_executable,
            ..Default::default()
        };
        let legacy = Secrets {
            telegram_token: "7:old_fixture_token".into(),
            ..Default::default()
        };
        advanced::save_secrets(&db.connection, &mut old, None, Some(&legacy)).unwrap();
        let job = backup::save_job(
            &db.connection,
            backup::JobDraft {
                id: None,
                revision: None,
                name: "Legacy fixture".into(),
                source: source.to_string_lossy().into(),
                destination: dest.to_string_lossy().into(),
                kind: "local".into(),
                schedule_automatic: false,
                wake_computer: false,
                deletion_guard_enabled: true,
                deletion_guard_min_files: 10,
                retention_days: 30,
                archive_mode: "days".into(),
                schedule_kind: "daily".into(),
                schedule_every: 1,
                daily_minute: 0,
                missed_run: "skip".into(),
                advanced: old,
                secrets: Some(legacy),
            },
        )
        .unwrap();
        assert!(!get(&db.connection, 0).unwrap().telegram_configured);
        assert_eq!(
            effective(&db.connection, &job.advanced)
                .unwrap()
                .0
                .telegram_chat,
            "777"
        );
        let mut only_mail = fields();
        only_mail.telegram_chat.clear();
        save(
            &db.connection,
            Update {
                fields: only_mail,
                smtp_password: "fixture_smtp".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let view = import_job(&db.connection, &job.id).unwrap();
        assert!(view.telegram_configured && view.smtp_configured);
        assert_eq!(view.fields.telegram_chat, "777");
        assert_eq!(view.fields.smtp_to, "recipient@example.invalid");
        let after = backup::get_job(&db.connection, &job.id).unwrap();
        assert_eq!(job.revision, after.revision);
        assert_eq!(job.schedule_enabled, after.schedule_enabled);
        assert_eq!(job.advanced, after.advanced);
    }
}
