use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub id: i64,
    pub github_repo_id: i64,
    pub account_id: i64,
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub private: bool,
    pub archived: bool,
    pub default_branch: Option<String>,
    pub ssh_url: String,
    pub html_url: String,
    pub language: Option<String>,
    pub permissions_snapshot: Option<String>,
    pub last_synced_at: Option<DateTime<Utc>>,
}
