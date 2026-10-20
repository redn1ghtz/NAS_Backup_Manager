//! Keep manual filesystem work alive when its window is closed, without an idle daemon.
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::Manager;

#[derive(Default)]
pub struct WorkState(AtomicUsize);

pub fn window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        if window.state::<WorkState>().active() {
            api.prevent_close();
            // Native tray/EXE activation uses ShowWindowAsync. Tao's cached
            // VISIBLE flag can still be false after that external restore, so a
            // second hide would be optimized away. Reconcile the toolkit flag
            // first; both operations run in this same native event callback.
            if window.is_visible().is_ok_and(|visible| visible) {
                let _ = window.show();
            }
            // If hiding fails the window stays visible and work continues.
            let _ = window.hide();
        }
    }
}

impl WorkState {
    pub fn active(&self) -> bool {
        self.0.load(Ordering::SeqCst) != 0
    }
    fn begin(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn finish(&self) -> bool {
        self.0.fetch_sub(1, Ordering::SeqCst) == 1
    }
}

pub struct WorkGuard(tauri::AppHandle);
impl WorkGuard {
    pub fn begin(app: &tauri::AppHandle) -> Self {
        app.state::<WorkState>().begin();
        Self(app.clone())
    }
}
impl Drop for WorkGuard {
    fn drop(&mut self) {
        if !self.0.state::<WorkState>().finish() {
            return;
        }
        let app = self.0.clone();
        // Serialize the visibility check with close/show window messages. Reopening
        // the interface before completion keeps it open for the result.
        let _ = self.0.run_on_main_thread(move || {
            if !app.state::<WorkState>().active()
                && app
                    .get_webview_window("main")
                    .is_some_and(|window| window.is_visible().is_ok_and(|visible| !visible))
            {
                app.exit(0);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closing_can_exit_only_after_all_workers_finish() {
        let state = WorkState::default();
        assert!(!state.active());
        state.begin();
        state.begin();
        assert!(state.active());
        assert!(!state.finish());
        assert!(state.active());
        assert!(state.finish());
        assert!(!state.active());
        // A new command queued before the exit callback must prevent exit.
        state.begin();
        assert!(state.active());
        assert!(state.finish());
    }
}
