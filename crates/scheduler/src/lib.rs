//! Platform adapters remain outside the portable engine.
pub mod registration;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchedulerBackend {
    WindowsTaskScheduler,
    SystemdUserTimer,
    Launchd,
}

/// Compile-target candidate only. Runtime availability must be checked at registration.
pub fn candidate_backend() -> Option<SchedulerBackend> {
    if cfg!(target_os = "windows") {
        Some(SchedulerBackend::WindowsTaskScheduler)
    } else if cfg!(target_os = "linux") {
        Some(SchedulerBackend::SystemdUserTimer)
    } else if cfg!(target_os = "macos") {
        Some(SchedulerBackend::Launchd)
    } else {
        None
    }
}
