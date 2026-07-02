use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub id: i64,
    pub target_type: TargetType,
    pub alias: String,
    pub os: OsType,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub auth_method: Option<AuthMethod>,
    pub auth_ref: Option<String>,
    pub key_base_dir: String,
    pub status: TargetStatus,
    pub host_key_fingerprint: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_checked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TargetType {
    Local,
    Remote,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OsType {
    MacOs,
    Linux,
    Windows,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuthMethod {
    Password,
    SshKey,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TargetStatus {
    Active,
    Unreachable,
    AuthFailed,
    Unknown,
}

impl std::fmt::Display for TargetType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TargetType::Local => write!(f, "local"),
            TargetType::Remote => write!(f, "remote"),
        }
    }
}

impl std::str::FromStr for TargetType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "local" => Ok(TargetType::Local),
            "remote" => Ok(TargetType::Remote),
            _ => Err(format!("Invalid target type: {}", s)),
        }
    }
}

impl std::fmt::Display for OsType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OsType::MacOs => write!(f, "macos"),
            OsType::Linux => write!(f, "linux"),
            OsType::Windows => write!(f, "windows"),
            OsType::Unknown => write!(f, "unknown"),
        }
    }
}

impl std::str::FromStr for OsType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "macos" => Ok(OsType::MacOs),
            "linux" => Ok(OsType::Linux),
            "windows" => Ok(OsType::Windows),
            "unknown" => Ok(OsType::Unknown),
            _ => Err(format!("Invalid os type: {}", s)),
        }
    }
}

impl std::fmt::Display for AuthMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthMethod::Password => write!(f, "password"),
            AuthMethod::SshKey => write!(f, "ssh_key"),
        }
    }
}

impl std::str::FromStr for AuthMethod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "password" => Ok(AuthMethod::Password),
            "ssh_key" => Ok(AuthMethod::SshKey),
            _ => Err(format!("Invalid auth method: {}", s)),
        }
    }
}

impl std::fmt::Display for TargetStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TargetStatus::Active => write!(f, "active"),
            TargetStatus::Unreachable => write!(f, "unreachable"),
            TargetStatus::AuthFailed => write!(f, "auth_failed"),
            TargetStatus::Unknown => write!(f, "unknown"),
        }
    }
}

impl std::str::FromStr for TargetStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(TargetStatus::Active),
            "unreachable" => Ok(TargetStatus::Unreachable),
            "auth_failed" => Ok(TargetStatus::AuthFailed),
            "unknown" => Ok(TargetStatus::Unknown),
            _ => Err(format!("Invalid target status: {}", s)),
        }
    }
}
