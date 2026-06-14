use serde::{Deserialize, Serialize};
use std::fmt;

/// A short, stable identifier for an in-flight operation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperationId(pub String);

impl fmt::Display for OperationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for OperationId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

/// Progress payload for a single operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    /// Percent complete in the range [0, 100].
    pub percent: u8,
}

/// Something that can receive progress checkpoints during an async operation.
pub trait ProgressReporter: Send + Sync {
    /// Report the next checkpoint. `percent` should be monotonically increasing
    /// and stay in [0, 100].
    fn report(&self, operation: OperationId, percent: u8);
}

/// No-op reporter for tests and code paths that do not need progress visibility.
pub struct NoOpProgressReporter;

impl ProgressReporter for NoOpProgressReporter {
    fn report(&self, _operation: OperationId, _percent: u8) {}
}
