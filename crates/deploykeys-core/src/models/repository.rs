use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub id: i64,
    pub github_repo_id: i64,
    pub installation_id: i64,
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub private: bool,
    pub archived: bool,
    pub default_branch: Option<String>,
    pub ssh_url: String,
    pub html_url: String,
    pub permissions_snapshot: Option<String>,
    pub last_synced_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubInstallation {
    pub id: i64,
    pub github_installation_id: i64,
    pub account_id: i64,
    pub account_owner: String,
    pub account_type: AccountType,
    pub permissions_snapshot: Option<String>,
    pub repository_selection: RepositorySelection,
    pub last_synced_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AccountType {
    User,
    Organization,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RepositorySelection {
    All,
    Selected,
}

impl std::fmt::Display for AccountType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccountType::User => write!(f, "user"),
            AccountType::Organization => write!(f, "org"),
        }
    }
}

impl std::str::FromStr for AccountType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(AccountType::User),
            "org" => Ok(AccountType::Organization),
            _ => Err(format!("Invalid account type: {}", s)),
        }
    }
}

impl std::fmt::Display for RepositorySelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RepositorySelection::All => write!(f, "all"),
            RepositorySelection::Selected => write!(f, "selected"),
        }
    }
}

impl std::str::FromStr for RepositorySelection {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "all" => Ok(RepositorySelection::All),
            "selected" => Ok(RepositorySelection::Selected),
            _ => Err(format!("Invalid repository selection: {}", s)),
        }
    }
}
