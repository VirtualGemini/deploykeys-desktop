use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: i64,
    pub github_user_id: i64,
    pub login: String,
    pub avatar_url: Option<String>,
    pub auth_type: AuthType,
    pub token_ref: String,
    pub refresh_token_ref: Option<String>,
    pub token_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub last_login_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuthType {
    GitHubAppDeviceFlow,
    PersonalAccessToken,
}

impl std::fmt::Display for AuthType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthType::GitHubAppDeviceFlow => write!(f, "github_app_device_flow"),
            AuthType::PersonalAccessToken => write!(f, "personal_access_token"),
        }
    }
}

impl std::str::FromStr for AuthType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "github_app_device_flow" => Ok(AuthType::GitHubAppDeviceFlow),
            "personal_access_token" => Ok(AuthType::PersonalAccessToken),
            _ => Err(format!("Invalid auth type: {}", s)),
        }
    }
}
