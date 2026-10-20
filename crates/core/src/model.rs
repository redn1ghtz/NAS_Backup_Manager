use std::path::{Component, Path, PathBuf};

/// Application-generated stable identifier, also safe as a single path component.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct JobId(String);

impl JobId {
    pub fn parse(value: &str) -> Result<Self, ValidationError> {
        // UUID-shaped IDs avoid Windows reserved names and path traversal.
        if value.len() != 36
            || !value.bytes().enumerate().all(|(i, c)| {
                if [8, 13, 18, 23].contains(&i) {
                    c == b'-'
                } else {
                    c.is_ascii_hexdigit()
                }
            })
        {
            return Err(ValidationError::InvalidJobId);
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DestinationKind {
    Local,
    Network,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissedRunPolicy {
    Skip,
    RunOnceWhenAvailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DailyTime {
    hour: u8,
    minute: u8,
}

impl DailyTime {
    pub fn new(hour: u8, minute: u8) -> Result<Self, ValidationError> {
        if hour > 23 || minute > 59 {
            return Err(ValidationError::InvalidTime);
        }
        Ok(Self { hour, minute })
    }
    pub fn hour(self) -> u8 {
        self.hour
    }
    pub fn minute(self) -> u8 {
        self.minute
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetentionDays(u32);

impl RetentionDays {
    pub fn new(days: u32) -> Result<Self, ValidationError> {
        if days == 0 {
            return Err(ValidationError::InvalidRetention);
        }
        Ok(Self(days))
    }
    pub fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug)]
pub struct Schedule {
    pub enabled: bool,
    pub daily_time: DailyTime,
    pub missed_run: MissedRunPolicy,
}

/// Draft configuration only: never sufficient authority to perform I/O.
#[derive(Clone, Debug)]
pub struct Job {
    pub id: JobId,
    pub name: String,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub destination_kind: DestinationKind,
    pub retention: RetentionDays,
    pub enabled: bool,
    pub schedule: Schedule,
    /// Increment on any edit. Preview consent must bind to this revision.
    pub revision: u64,
}

impl Job {
    /// Syntax checks only. Storage must canonicalize and verify live identities.
    pub fn validate_syntax(&self) -> Result<(), ValidationError> {
        if self.name.trim().is_empty() {
            return Err(ValidationError::EmptyName);
        }
        if self.revision == 0 {
            return Err(ValidationError::InvalidRevision);
        }
        for path in [&self.source, &self.destination] {
            if !path.is_absolute() {
                return Err(ValidationError::AbsolutePathRequired);
            }
            if path.components().any(|c| matches!(c, Component::ParentDir)) {
                return Err(ValidationError::ParentTraversal);
            }
        }
        Ok(())
    }
}

/// Defense in depth for persisted paths; Windows aliases need additional adapter checks.
pub fn validate_relative_path(path: &Path) -> Result<(), ValidationError> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(ValidationError::UnsafeRelativePath);
    }
    // Portable archive paths reject alternate data streams, trailing-dot aliases,
    // reserved device names, and separators that are ordinary chars on Unix.
    for part in path.components() {
        let text = part
            .as_os_str()
            .to_str()
            .ok_or(ValidationError::UnsafeRelativePath)?;
        let stem = text.split('.').next().unwrap_or("").to_ascii_uppercase();
        let reserved = ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"].contains(&stem.as_str())
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'));
        if reserved
            || text.ends_with(['.', ' '])
            || text.chars().any(|c| {
                c.is_control() || [':', '\\', '/', '<', '>', '"', '|', '?', '*'].contains(&c)
            })
        {
            return Err(ValidationError::UnsafeRelativePath);
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidationError {
    InvalidJobId,
    InvalidTime,
    InvalidRetention,
    EmptyName,
    InvalidRevision,
    AbsolutePathRequired,
    ParentTraversal,
    UnsafeRelativePath,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_times_and_accepts_day_boundaries() {
        assert!(DailyTime::new(0, 0).is_ok());
        assert!(DailyTime::new(23, 59).is_ok());
        assert!(DailyTime::new(24, 0).is_err());
        assert!(DailyTime::new(12, 60).is_err());
    }

    #[test]
    fn retention_requires_a_recovery_window() {
        assert!(RetentionDays::new(0).is_err());
        assert_eq!(RetentionDays::new(3650).unwrap().get(), 3650);
    }

    #[test]
    fn ids_cannot_be_paths_or_reserved_devices() {
        for id in ["../job", "CON", "", "x/y", "job:stream"] {
            assert!(JobId::parse(id).is_err());
        }
        assert_eq!(
            JobId::parse("AAAAAAAA-0000-4000-8000-000000000001")
                .unwrap()
                .as_str(),
            "aaaaaaaa-0000-4000-8000-000000000001"
        );
    }

    #[test]
    fn archive_paths_are_portable_and_confined() {
        for path in [
            "",
            "../file",
            "/file",
            "a/../b",
            "file:stream",
            "CON.txt",
            "a.",
            "a ",
            "LPT1",
        ] {
            assert!(validate_relative_path(Path::new(path)).is_err(), "{path}");
        }
        assert!(validate_relative_path(Path::new("Документы/report.xlsx")).is_ok());
    }
}
