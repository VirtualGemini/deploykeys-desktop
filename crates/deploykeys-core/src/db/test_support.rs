//! Shared fixtures for database-backed tests: a migrated temp database and
//! seed rows satisfying the foreign-key chain
//! account -> installation -> repository / target -> key_binding.

use super::Database;
use crate::models::{
    Account, AccountType, AuthType, GitHubInstallation, OsType, Repository, RepositorySelection,
    Target, TargetStatus, TargetType,
};
use chrono::Utc;
use tempfile::TempDir;

/// Open a fresh, fully migrated database in a temp directory.
///
/// Keep the returned `TempDir` alive for the duration of the test.
pub(crate) async fn test_db() -> (TempDir, Database) {
    let dir = TempDir::new().expect("create temp dir");
    let db = Database::new(&dir.path().join("test.db"))
        .await
        .expect("open database");
    db.run_migrations().await.expect("run migrations");
    (dir, db)
}

/// Insert an account with login `seeded` (token_ref `github_token_seeded`).
pub(crate) async fn seed_account(db: &Database) -> i64 {
    let account = Account {
        id: 0,
        github_user_id: 4242,
        login: "seeded".to_string(),
        avatar_url: None,
        auth_type: AuthType::GitHubAppDeviceFlow,
        token_ref: "github_token_seeded".to_string(),
        refresh_token_ref: None,
        token_expires_at: None,
        created_at: Utc::now(),
        last_login_at: Utc::now(),
    };
    db.accounts().create(&account).await.expect("seed account")
}

pub(crate) async fn seed_installation(db: &Database, account_id: i64) -> i64 {
    let installation = GitHubInstallation {
        id: 0,
        github_installation_id: 9001,
        account_id,
        account_owner: "owner".to_string(),
        account_type: AccountType::User,
        permissions_snapshot: None,
        repository_selection: RepositorySelection::All,
        last_synced_at: None,
    };
    db.installations()
        .create(&installation)
        .await
        .expect("seed installation")
}

/// Insert repository `owner/repo` under the given installation.
pub(crate) async fn seed_repository(db: &Database, installation_id: i64) -> i64 {
    let repo = Repository {
        id: 0,
        github_repo_id: 1337,
        installation_id,
        owner: "owner".to_string(),
        name: "repo".to_string(),
        full_name: "owner/repo".to_string(),
        private: true,
        archived: false,
        default_branch: Some("main".to_string()),
        ssh_url: "git@github.com:owner/repo.git".to_string(),
        html_url: "https://github.com/owner/repo".to_string(),
        permissions_snapshot: None,
        last_synced_at: None,
    };
    db.repositories()
        .create(&repo)
        .await
        .expect("seed repository")
}

pub(crate) async fn seed_target(db: &Database) -> i64 {
    let target = Target {
        id: 0,
        target_type: TargetType::Local,
        alias: "Seeded Target".to_string(),
        os: OsType::Linux,
        host: None,
        port: None,
        username: None,
        auth_method: None,
        auth_ref: None,
        key_base_dir: "/tmp/keys".to_string(),
        status: TargetStatus::Active,
        host_key_fingerprint: None,
        created_at: Utc::now(),
        last_checked_at: None,
    };
    db.targets().create(&target).await.expect("seed target")
}
