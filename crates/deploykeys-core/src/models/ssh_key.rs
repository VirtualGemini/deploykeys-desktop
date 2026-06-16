use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::KeyAlgorithm;

/// A standalone SSH key pair managed locally, independent of GitHub deploy keys.
/// Stored in `~/.ssh/deploykeys/<directory>/` with isolated directory per key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshKey {
    pub id: i64,
    /// Directory used on disk (e.g., "prod-server-key"). Unique per key.
    pub directory: String,
    pub algorithm: KeyAlgorithm,
    pub public_key: String,
    pub public_key_fingerprint: String,
    /// Full path to private key file
    pub private_key_path: String,
    /// Full path to public key file
    pub public_key_path: String,
    /// Comment appended to the public key line (conventionally an email), used
    /// to identify the key's owner. Stored verbatim — not synthesized.
    pub comment: String,
    /// Free-form user note shown in the keys list.
    pub remark: String,
    /// Target this key is associated with (local machine for Phase 1)
    pub target_id: i64,
    pub created_at: DateTime<Utc>,
}
