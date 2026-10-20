//! Ports for subsequent implementation. A validated Job alone grants no write authority.
use crate::model::Job;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorCode {
    InvalidPath,
    UnsafeOverlap,
    SourceUnavailable,
    IncompleteScan,
    DestinationUnavailable,
    IdentityMismatch,
    OwnershipMismatch,
    MassDeletion,
    PreviewRequired,
    Busy,
    ArchiveFailed,
    CopyFailed,
    RestoreCollision,
    RecoveryRequired,
    UnsupportedPlatform,
}

#[derive(Clone, Debug)]
pub struct BackupError {
    pub code: ErrorCode,
    /// Sanitized context; localized messages are selected using code.
    pub context: String,
}

/// Identity comes from the OS mount/volume adapter, never from a path string alone.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DestinationIdentity {
    pub volume_or_share_id: String,
    pub mount_root: std::path::PathBuf,
    pub ownership_nonce: String,
}

pub trait DestinationVerifier {
    /// Recheck before mutations, fail closed if identity cannot be established.
    fn verify(&self, job: &Job, expected: &DestinationIdentity) -> Result<(), BackupError>;
}

pub trait Scheduler {
    /// Register the headless CLI with the OS, only after successful preview consent.
    fn register(&self, job: &Job, cli: &std::path::Path) -> Result<(), BackupError>;
    fn unregister(&self, job: &Job) -> Result<(), BackupError>;
}
