#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use nas_backup_storage::database::Database;
mod commands;
mod lifecycle;
mod state_observer;
use std::sync::Mutex;
use tauri::Manager;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AppStatus {
    version: &'static str,
    schema_version: i64,
    job_count: i64,
    language: String,
    execution_available: bool,
    wake_supported: bool,
    notifications_enabled: bool,
}

fn database(app: &tauri::AppHandle) -> Result<Database, String> {
    Database::open(&commands::state_path(app)?).map_err(|_| "database_unavailable".into())
}

#[tauri::command]
async fn get_app_status(app: tauri::AppHandle) -> Result<AppStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = database(&app)?;
        Ok(AppStatus {
            version: env!("CARGO_PKG_VERSION"),
            schema_version: db.schema_version().map_err(|_| "database_unavailable")?,
            job_count: db.active_job_count().map_err(|_| "database_unavailable")?,
            language: db.language().map_err(|_| "database_unavailable")?,
            execution_available: true,
            wake_supported: cfg!(windows),
            notifications_enabled: db
                .notifications_enabled()
                .map_err(|_| "database_unavailable")?,
        })
    })
    .await
    .map_err(|_| "worker_failed".to_string())?
}

#[tauri::command]
async fn set_language(app: tauri::AppHandle, language: String) -> Result<(), String> {
    if !["ru", "en"].contains(&language.as_str()) {
        return Err("invalid_language".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        database(&app)?
            .set_language(&language)
            .map_err(|_| "database_unavailable".into())
    })
    .await
    .map_err(|_| "worker_failed".to_string())?
}

#[tauri::command]
async fn set_notifications(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        database(&app)?
            .set_notifications_enabled(enabled)
            .map_err(|_| "database_unavailable".into())
    })
    .await
    .map_err(|_| "worker_failed".to_string())?
}

// SQLite data_version uses one persistent local connection to detect headless commits.
#[tauri::command]
async fn state_changed(app: tauri::AppHandle) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let observer = app.state::<Mutex<state_observer::StateObserver>>();
        let mut guard = observer.lock().map_err(|_| "worker_failed".to_string())?;
        guard.changed(&commands::state_path(&app)?)
    })
    .await
    .map_err(|_| "worker_failed".to_string())?
}

// Only these two fixed HTTPS destinations may be opened by the webview.
#[tauri::command]
fn open_project_link(target: &str) -> Result<(), String> {
    let url = match target {
        "author" => "https://github.com/redn1ghtz",
        "project" => "https://github.com/redn1ghtz/NAS_Backup_Manager",
        _ => return Err("invalid_link".into()),
    };
    #[cfg(windows)]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let mut command = std::process::Command::new(
            std::path::PathBuf::from(std::env::var_os("SystemRoot").ok_or("link_open_failed")?)
                .join("System32/rundll32.exe"),
        );
        command
            .creation_flags(0x08000000)
            .arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(not(any(windows, target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    command
        .arg(url)
        .spawn()
        .map_err(|_| "link_open_failed".to_string())?;
    Ok(())
}

fn main() {
    if let Ok(executable) = std::env::current_exe() {
        if nas_backup_storage::desktop_activity::focus_existing(&executable).unwrap_or(false) {
            return;
        }
    }

    tauri::Builder::default()
        .manage(lifecycle::WorkState::default())
        .manage(Mutex::new(state_observer::StateObserver::default()))
        .on_window_event(lifecycle::window_event)
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_app_status,
            state_changed,
            set_language,
            set_notifications,
            open_project_link,
            commands::backup_command
        ])
        .run(tauri::generate_context!())
        .expect("Desktop runtime failed");
}
