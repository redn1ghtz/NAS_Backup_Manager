use nas_backup_core::backup::{self, Failure, JobDraft};
use serde_json::Value;
use tauri::{Emitter, Manager};

#[derive(serde::Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Request {
    NotificationSettings {
        #[serde(default)]
        offset: i64,
    },
    SaveNotificationSettings {
        update: nas_backup_core::notification_settings::Update,
    },
    ImportNotificationSettings {
        id: String,
    },
    Activity,
    ToolPaths,
    PrepareCloud {
        provider: String,
    },
    AuthorizeCloud {
        id: String,
    },
    CancelCloudAuthorization {
        id: String,
    },
    CloudConnectionStatus {
        id: String,
    },
    GenerateIdentity {
        age_executable: String,
        path: String,
    },
    DecryptArchive {
        id: String,
        directory: String,
    },
    ReadIdentity {
        path: String,
    },
    CloudCapacity {
        id: String,
        #[serde(default)]
        force: bool,
    },
    VerifyIntegrity {
        id: String,
    },
    DecryptFile {
        id: String,
        relative: String,
        directory: String,
    },
    Cancel {
        id: String,
        execution_id: String,
    },
    NetworkProfile {
        path: String,
    },
    SaveGuestNetworkAccess {
        path: String,
    },
    NetworkStatus {
        path: String,
    },
    SaveNetworkCredentials {
        path: String,
        username: String,
        password: String,
    },
    ForgetNetworkCredentials {
        path: String,
    },

    FileHistory {
        offset: i64,
        errors_only: bool,
    },
    Dashboard {
        id: Option<String>,
    },
    ListJobs {
        offset: i64,
    },
    ArchiveJobs {
        offset: i64,
    },
    NextRun {
        id: String,
    },
    SaveJob {
        draft: Box<JobDraft>,
    },
    Preview {
        id: String,
    },
    Run {
        id: String,
        preview_id: String,
        consent: bool,
    },
    Operations {
        id: String,
        offset: i64,
    },
    Archive {
        id: String,
        offset: i64,
    },
    Restore {
        id: String,
        #[serde(default)]
        collision_token: Option<String>,
    },
    Cleanup {
        id: String,
        consent: bool,
    },
    History {
        offset: i64,
        #[serde(default)]
        errors_only: bool,
    },
    Pause {
        id: String,
    },
    Remove {
        id: String,
    },
    Capacity {
        id: String,
    },
    Schedule {
        id: String,
        enabled: bool,
    },
}
fn failure(code: &str) -> Failure {
    Failure {
        code: code.into(),
        detail: String::new(),
    }
}
fn value<T: serde::Serialize>(data: T) -> Result<Value, Failure> {
    serde_json::to_value(data).map_err(|_| failure("serialization"))
}
pub fn state_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let executable = std::env::current_exe().map_err(|_| "state_directory_unavailable")?;
    let directory = if executable
        .parent()
        .is_some_and(|p| p.join("portable.flag").is_file())
    {
        executable.parent().unwrap().join("data")
    } else {
        app.path()
            .app_data_dir()
            .map_err(|_| "state_directory_unavailable")?
    };
    std::fs::create_dir_all(&directory).map_err(|_| "state_directory_unavailable")?;
    Ok(directory.join("state.sqlite"))
}
#[tauri::command]
pub async fn backup_command(app: tauri::AppHandle, request: Request) -> Result<Value, Failure> {
    let work = matches!(
        request,
        Request::DecryptArchive { .. }
            | Request::VerifyIntegrity { .. }
            | Request::PrepareCloud { .. }
            | Request::AuthorizeCloud { .. }
            | Request::DecryptFile { .. }
            | Request::Run { .. }
            | Request::Cancel { .. }
            | Request::Restore { .. }
            | Request::Cleanup { .. }
            | Request::Preview { .. }
            | Request::SaveNotificationSettings { .. }
            | Request::ImportNotificationSettings { .. }
            | Request::SaveJob { .. }
            | Request::Pause { .. }
            | Request::Remove { .. }
            | Request::Schedule { .. }
            | Request::SaveGuestNetworkAccess { .. }
            | Request::SaveNetworkCredentials { .. }
            | Request::ForgetNetworkCredentials { .. }
    )
    .then(|| crate::lifecycle::WorkGuard::begin(&app));
    tauri::async_runtime::spawn_blocking(move || {
        let _work = work;
        let path = state_path(&app).map_err(|_| failure("state_directory_unavailable"))?;
        let database =
            nas_backup_storage::database::Database::open(&path).map_err(|_| failure("database"))?;
        let db = &database.connection;
        match request {
            Request::NotificationSettings {offset} => value(nas_backup_core::notification_settings::get(db,offset)?),
            Request::SaveNotificationSettings {update} => value(nas_backup_core::notification_settings::save(db,update)?),
            Request::ImportNotificationSettings {id} => value(nas_backup_core::notification_settings::import_job(db,&id)?),
            Request::PrepareCloud {provider} => {
                let exe=std::env::current_exe().map_err(|_|failure("tool_unavailable"))?;
                let tool=exe.parent().ok_or_else(||failure("tool_unavailable"))?.join("tools").join(if cfg!(windows){"rclone.exe"}else{"rclone"});
                value(nas_backup_core::cloud_setup::prepare(db,&provider,tool.to_str().ok_or_else(||failure("tool_unavailable"))?)?)
            }
            Request::AuthorizeCloud {id} => value(nas_backup_core::cloud_setup::authorize(db,&id)?),
            Request::CancelCloudAuthorization {id} => value(nas_backup_core::cloud_setup::cancel(db,&id)?),
            Request::CloudConnectionStatus {id} => value(nas_backup_core::cloud_setup::status(db,&id)?),
            Request::GenerateIdentity {age_executable,path} => {
                let age=nas_backup_core::advanced::executable_path(&age_executable)?;
                let keygen=age.with_file_name(if cfg!(windows){"age-keygen.exe"}else{"age-keygen"});
                let (recipient,identity)=nas_backup_core::remote::generate_identity(keygen.to_str().ok_or_else(||failure("tool_unavailable"))?,std::path::Path::new(&path))?;
                value(serde_json::json!({"recipient":recipient,"identity":identity}))
            }
            Request::DecryptArchive {id,directory} => value(backup::decrypt_archive(db,&id,std::path::Path::new(&directory))?),
            Request::ToolPaths => {
                let exe=std::env::current_exe().map_err(|_|failure("tool_unavailable"))?;
                let folder=exe.parent().ok_or_else(||failure("tool_unavailable"))?.join("tools");
                let curl=std::env::var_os("SystemRoot").map(|p|std::path::PathBuf::from(p).join("System32/curl.exe")).unwrap_or_else(||folder.join("curl"));
                value(serde_json::json!({"age":folder.join(if cfg!(windows){"age.exe"}else{"age"}),"rclone":folder.join(if cfg!(windows){"rclone.exe"}else{"rclone"}),"curl":curl}))
            }
            Request::ReadIdentity {path} => {
                let p=std::path::Path::new(&path);let metadata=std::fs::symlink_metadata(p).map_err(|_|failure("invalid_encryption_key"))?;
                if !p.is_absolute()||!metadata.is_file()||metadata.len()>65536{return Err(failure("invalid_encryption_key"));}
                let key=std::fs::read_to_string(p).map_err(|_|failure("invalid_encryption_key"))?;
                if !key.lines().any(|line|line.starts_with("AGE-SECRET-KEY-"))||key.lines().any(|line|!line.is_empty()&&!line.starts_with('#')&&!line.starts_with("AGE-SECRET-KEY-")){return Err(failure("invalid_encryption_key"));}
                value(key)
            }
            Request::CloudCapacity {id,force} => value(nas_backup_core::remote::capacity(db,&backup::get_job(db,&id)?,force)?),
            Request::VerifyIntegrity {id} => value(backup::verify_integrity(db,&id)?),
            Request::DecryptFile {id,relative,directory} => value(backup::decrypt_file(db,&id,&relative,std::path::Path::new(&directory))?),
            Request::Activity => value(nas_backup_core::activity::list(db)?),
            Request::Cancel { id, execution_id } => value(
                nas_backup_core::activity::request_cancel(db, &id, &execution_id)?,
            ),
            Request::NetworkProfile { path } => {
                value(nas_backup_storage::network::access_mode(&path).map_err(failure)?)
            }
            Request::SaveGuestNetworkAccess { path } => {
                nas_backup_storage::network::save_guest_access(&path).map_err(failure)?;
                value(())
            }
            Request::NetworkStatus { path } => {
                value(nas_backup_storage::network::credentials_saved(&path).map_err(failure)?)
            }
            Request::SaveNetworkCredentials {
                path,
                username,
                password,
            } => {
                nas_backup_storage::network::save_credentials(&path, &username, &password)
                    .map_err(failure)?;
                value(())
            }
            Request::ForgetNetworkCredentials { path } => {
                nas_backup_storage::network::forget_credentials(&path).map_err(failure)?;
                value(())
            }
            Request::FileHistory {
                offset,
                errors_only,
            } => value(backup::file_history(db, offset, errors_only)?),
            Request::Dashboard { id } => value(backup::dashboard(db, id.as_deref())?),
            Request::ListJobs { offset } => value(backup::list_jobs(db, offset)?),
            Request::ArchiveJobs { offset } => value(backup::archive_jobs(db, offset)?),
            Request::NextRun { id } => value(
                nas_backup_scheduler::registration::next_run(&backup::get_job(db, &id)?)
                    .map_err(|code| failure(&code))?,
            ),
            Request::SaveJob { draft } => value(backup::save_job(db, *draft)?),
            Request::Preview { id } => value(backup::preview(db, &id)?),
            Request::Run {
                id,
                preview_id,
                consent,
            } => {
                let job = backup::get_job(db, &id)?;
                let mut feedback = nas_backup_core::activity::Feedback::new(db, &job, &path);
                let outcome = backup::execute_progress(db, &id, &preview_id, consent, |progress| {
                    feedback.progress(&progress);
                    let _ = app.emit("backup-progress", progress);
                });
                feedback.finish(outcome.as_ref().err());
                let result = outcome?;
                let job = backup::get_job(db, &id)?;
                let schedule_error = if job.schedule_pending && job.schedule_automatic {
                    set_schedule(&database, &job, &path, true)
                        .err()
                        .map(|error| error.code)
                } else {
                    None
                };
                value(serde_json::json!({"result":result,"scheduleError":schedule_error}))
            }
            Request::Operations { id, offset } => value(backup::operations(db, &id, offset)?),
            Request::Archive { id, offset } => value(backup::list_archive(db, &id, offset)?),
            Request::Restore {
                id,
                collision_token,
            } => value(backup::restore_confirmed(
                db,
                &id,
                collision_token.as_deref(),
            )?),
            Request::Cleanup { id, consent } => value(backup::cleanup(db, &id, consent)?),
            Request::History {
                offset,
                errors_only,
            } => value(backup::history_filtered(db, offset, errors_only)?),
            Request::Pause { id } => {
                let job = backup::get_job(db, &id)?;
                db.execute(
                    "UPDATE jobs SET schedule_enabled=0,enabled=0,schedule_pending=0 WHERE id=?1",
                    [&id],
                )
                .map_err(|_| failure("database"))?;
                if job.schedule_enabled {
                    nas_backup_scheduler::registration::unregister(&job)
                        .map_err(|s| failure(&s))?;
                }
                value(())
            }
            Request::Remove { id } => value(backup::remove_job(db, &id)?),
            Request::Capacity { id } => value(backup::capacity(db, &id)?),
            Request::Schedule { id, enabled } => {
                let job = backup::get_job(db, &id)?;
                set_schedule(&database, &job, &path, enabled)?;
                value(())
            }
        }
    })
    .await
    .map_err(|_| failure("worker_failed"))?
}

fn set_schedule(
    database: &nas_backup_storage::database::Database,
    job: &backup::SavedJob,
    path: &std::path::Path,
    enabled: bool,
) -> Result<(), Failure> {
    let db = &database.connection;
    if enabled {
        let executable = std::env::current_exe().map_err(|_| failure("scheduler_cli_missing"))?;
        let cli = executable.with_file_name(if cfg!(windows) {
            "nas-backup.exe"
        } else {
            "nas-backup"
        });
        enable_schedule(database, job, path, &cli)?;
    } else {
        db.execute(
            "UPDATE jobs SET schedule_enabled=0,schedule_pending=0 WHERE id=?1",
            [&job.id],
        )
        .map_err(|_| failure("database"))?;
        if job.schedule_enabled {
            nas_backup_scheduler::registration::unregister(job).map_err(|code| failure(&code))?;
        }
    }
    Ok(())
}

fn enable_schedule(
    database: &nas_backup_storage::database::Database,
    job: &backup::SavedJob,
    path: &std::path::Path,
    cli: &std::path::Path,
) -> Result<(), Failure> {
    if !job.schedule_automatic {
        return Err(failure("manual_schedule"));
    }
    if !job.enabled {
        return Err(failure("preview_required"));
    }
    let db = &database.connection;
    nas_backup_scheduler::registration::register(job, cli, path).map_err(|code| failure(&code))?;
    match db.execute("UPDATE jobs SET schedule_enabled=1,schedule_pending=0 WHERE id=?1 AND revision=?2 AND schedule_automatic=1 AND enabled=1 AND removed_at IS NULL",(&job.id,job.revision)) {
            Ok(1)=>{},
            _=>{let _=nas_backup_scheduler::registration::unregister(job);return Err(failure("stale_job"));}
        }
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    struct Registered(Option<backup::SavedJob>);
    impl Drop for Registered {
        fn drop(&mut self) {
            if let Some(job) = self.0.as_ref() {
                let _ = nas_backup_scheduler::registration::unregister(job);
            }
        }
    }
    #[test]
    #[ignore = "explicit Windows scheduler access; disposable task and folders only"]
    fn approved_recurrence_activation_and_manual_refusal() {
        let cli = std::path::PathBuf::from(
            std::env::var_os("NAS_BACKUP_TEST_CLI").expect("absolute built CLI required"),
        );
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&destination).unwrap();
        std::fs::write(source.join("fixture.txt"), "test fixture").unwrap();
        let state = temp.path().join("state.sqlite");
        let database = nas_backup_storage::database::Database::open(&state).unwrap();
        let mut draft = JobDraft {
            advanced: Default::default(),
            secrets: None,
            id: None,
            revision: None,
            name: "Auto activation QA".into(),
            source: source.to_string_lossy().into(),
            destination: destination.to_string_lossy().into(),
            kind: "local".into(),
            schedule_automatic: true,
            wake_computer: false,
            deletion_guard_enabled: true,
            deletion_guard_min_files: 10,
            retention_days: 30,
            archive_mode: "days".into(),
            schedule_kind: "minutes".into(),
            schedule_every: 10,
            daily_minute: 180,
            missed_run: "skip".into(),
        };
        let job = backup::save_job(&database.connection, draft.clone()).unwrap();
        assert_eq!(
            enable_schedule(&database, &job, &state, &cli)
                .unwrap_err()
                .code,
            "preview_required"
        );
        let plan = backup::preview(&database.connection, &job.id).unwrap();
        assert!(backup::execute(&database.connection, &job.id, &plan.id, false).is_err());
        assert!(
            !backup::get_job(&database.connection, &job.id)
                .unwrap()
                .schedule_enabled
        );
        backup::execute(&database.connection, &job.id, &plan.id, true).unwrap();
        let job = backup::get_job(&database.connection, &job.id).unwrap();
        assert!(job.schedule_pending);
        assert_eq!(
            enable_schedule(&database, &job, &state, &temp.path().join("missing.exe"))
                .unwrap_err()
                .code,
            "scheduler_cli_missing"
        );
        assert!(
            backup::get_job(&database.connection, &job.id)
                .unwrap()
                .schedule_pending
        );
        let mut registration = Registered(Some(job.clone()));
        enable_schedule(&database, &job, &state, &cli).unwrap();
        let enabled = backup::get_job(&database.connection, &job.id).unwrap();
        assert!(enabled.schedule_enabled && !enabled.schedule_pending);
        assert!(nas_backup_scheduler::registration::next_run(&enabled).is_ok());
        set_schedule(&database, &enabled, &state, false).unwrap();
        registration.0 = None;
        let disabled = backup::get_job(&database.connection, &job.id).unwrap();
        assert!(
            disabled.schedule_automatic && !disabled.schedule_pending && !disabled.schedule_enabled
        );
        draft.id = Some(job.id.clone());
        draft.revision = Some(job.revision);
        draft.source = job.source;
        draft.destination = job.destination;
        draft.schedule_automatic = false;
        let manual = backup::save_job(&database.connection, draft).unwrap();
        assert_eq!(
            enable_schedule(&database, &manual, &state, &cli)
                .unwrap_err()
                .code,
            "manual_schedule"
        );
        assert!(
            !backup::get_job(&database.connection, &manual.id)
                .unwrap()
                .schedule_enabled
        );
        drop(registration);
    }
}
