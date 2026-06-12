use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBinding {
    pub id: i64,
    pub repo_id: i64,
    pub target_id: i64,
    pub github_deploy_key_id: Option<i64>,
    pub deploy_key_title: String,
    pub algorithm: KeyAlgorithm,
    pub permission: DeployKeyPermission,
    pub public_key: String,
    pub public_key_fingerprint: String,
    pub private_key_path: String,
    pub private_key_residency: KeyResidency,
    pub status: KeyBindingStatus,
    pub created_at: DateTime<Utc>,
    pub last_verified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum KeyAlgorithm {
    Ed25519,
    Rsa2048,
    Rsa4096,
    EcdsaP256,
    EcdsaP384,
    EcdsaP521,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeployKeyPermission {
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum KeyResidency {
    Local,
    Remote,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum KeyBindingStatus {
    Pending,
    Active,
    Failed,
    Drifted,
    OrphanedLocal,
    OrphanedRemote,
    Revoked,
}

impl std::fmt::Display for KeyAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyAlgorithm::Ed25519 => write!(f, "ed25519"),
            KeyAlgorithm::Rsa2048 => write!(f, "rsa2048"),
            KeyAlgorithm::Rsa4096 => write!(f, "rsa4096"),
            KeyAlgorithm::EcdsaP256 => write!(f, "ecdsa_p256"),
            KeyAlgorithm::EcdsaP384 => write!(f, "ecdsa_p384"),
            KeyAlgorithm::EcdsaP521 => write!(f, "ecdsa_p521"),
        }
    }
}

impl std::str::FromStr for KeyAlgorithm {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ed25519" => Ok(KeyAlgorithm::Ed25519),
            "rsa2048" => Ok(KeyAlgorithm::Rsa2048),
            "rsa4096" => Ok(KeyAlgorithm::Rsa4096),
            "ecdsa_p256" => Ok(KeyAlgorithm::EcdsaP256),
            "ecdsa_p384" => Ok(KeyAlgorithm::EcdsaP384),
            "ecdsa_p521" => Ok(KeyAlgorithm::EcdsaP521),
            _ => Err(format!("Invalid key algorithm: {}", s)),
        }
    }
}

impl std::fmt::Display for DeployKeyPermission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeployKeyPermission::ReadOnly => write!(f, "read_only"),
            DeployKeyPermission::ReadWrite => write!(f, "read_write"),
        }
    }
}

impl std::str::FromStr for DeployKeyPermission {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "read_only" => Ok(DeployKeyPermission::ReadOnly),
            "read_write" => Ok(DeployKeyPermission::ReadWrite),
            _ => Err(format!("Invalid permission: {}", s)),
        }
    }
}

impl std::fmt::Display for KeyResidency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyResidency::Local => write!(f, "local"),
            KeyResidency::Remote => write!(f, "remote"),
        }
    }
}

impl std::str::FromStr for KeyResidency {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "local" => Ok(KeyResidency::Local),
            "remote" => Ok(KeyResidency::Remote),
            _ => Err(format!("Invalid residency: {}", s)),
        }
    }
}

impl std::fmt::Display for KeyBindingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyBindingStatus::Pending => write!(f, "pending"),
            KeyBindingStatus::Active => write!(f, "active"),
            KeyBindingStatus::Failed => write!(f, "failed"),
            KeyBindingStatus::Drifted => write!(f, "drifted"),
            KeyBindingStatus::OrphanedLocal => write!(f, "orphaned_local"),
            KeyBindingStatus::OrphanedRemote => write!(f, "orphaned_remote"),
            KeyBindingStatus::Revoked => write!(f, "revoked"),
        }
    }
}

impl std::str::FromStr for KeyBindingStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(KeyBindingStatus::Pending),
            "active" => Ok(KeyBindingStatus::Active),
            "failed" => Ok(KeyBindingStatus::Failed),
            "drifted" => Ok(KeyBindingStatus::Drifted),
            "orphaned_local" => Ok(KeyBindingStatus::OrphanedLocal),
            "orphaned_remote" => Ok(KeyBindingStatus::OrphanedRemote),
            "revoked" => Ok(KeyBindingStatus::Revoked),
            _ => Err(format!("Invalid status: {}", s)),
        }
    }
}
