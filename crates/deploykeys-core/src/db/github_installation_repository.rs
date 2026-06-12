use crate::db::{optional_timestamp, parse_enum_field};
use crate::models::GitHubInstallation;
use crate::Result;
use chrono::Utc;
use sqlx::SqlitePool;

pub struct GitHubInstallationRepository {
    pool: SqlitePool,
}

/// Raw `github_installations` row; converted to [`GitHubInstallation`] in one place.
struct InstallationRow {
    id: i64,
    github_installation_id: i64,
    account_id: i64,
    account_owner: String,
    account_type: String,
    permissions_snapshot: Option<String>,
    repository_selection: String,
    last_synced_at: Option<i64>,
}

impl TryFrom<InstallationRow> for GitHubInstallation {
    type Error = crate::Error;

    fn try_from(r: InstallationRow) -> Result<Self> {
        Ok(GitHubInstallation {
            id: r.id,
            github_installation_id: r.github_installation_id,
            account_id: r.account_id,
            account_owner: r.account_owner,
            account_type: parse_enum_field(&r.account_type, "account_type")?,
            permissions_snapshot: r.permissions_snapshot,
            repository_selection: parse_enum_field(
                &r.repository_selection,
                "repository_selection",
            )?,
            last_synced_at: optional_timestamp(r.last_synced_at),
        })
    }
}

impl GitHubInstallationRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, installation: &GitHubInstallation) -> Result<i64> {
        let account_type = installation.account_type.to_string();
        let repo_selection = installation.repository_selection.to_string();
        let last_synced_at = installation.last_synced_at.map(|t| t.timestamp());

        let result = sqlx::query!(
            r#"
            INSERT INTO github_installations (
                github_installation_id, account_id, account_owner, account_type,
                permissions_snapshot, repository_selection, last_synced_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
            installation.github_installation_id,
            installation.account_id,
            installation.account_owner,
            account_type,
            installation.permissions_snapshot,
            repo_selection,
            last_synced_at
        )
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    pub async fn find_by_id(&self, id: i64) -> Result<Option<GitHubInstallation>> {
        let row = sqlx::query_as!(
            InstallationRow,
            r#"
            SELECT id as "id!", github_installation_id, account_id, account_owner,
                   account_type, permissions_snapshot, repository_selection, last_synced_at
            FROM github_installations WHERE id = ?
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(GitHubInstallation::try_from).transpose()
    }

    pub async fn find_by_github_installation_id(
        &self,
        github_installation_id: i64,
    ) -> Result<Option<GitHubInstallation>> {
        let row = sqlx::query_as!(
            InstallationRow,
            r#"
            SELECT id as "id!", github_installation_id, account_id, account_owner,
                   account_type, permissions_snapshot, repository_selection, last_synced_at
            FROM github_installations WHERE github_installation_id = ?
            "#,
            github_installation_id
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(GitHubInstallation::try_from).transpose()
    }

    pub async fn list_by_account(&self, account_id: i64) -> Result<Vec<GitHubInstallation>> {
        let rows = sqlx::query_as!(
            InstallationRow,
            r#"
            SELECT id as "id!", github_installation_id, account_id, account_owner,
                   account_type, permissions_snapshot, repository_selection, last_synced_at
            FROM github_installations WHERE account_id = ? ORDER BY account_owner ASC
            "#,
            account_id
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(GitHubInstallation::try_from).collect()
    }

    pub async fn update_sync_time(&self, id: i64) -> Result<()> {
        let now = Utc::now().timestamp();

        sqlx::query!(
            "UPDATE github_installations SET last_synced_at = ? WHERE id = ?",
            now,
            id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete(&self, id: i64) -> Result<()> {
        sqlx::query!("DELETE FROM github_installations WHERE id = ?", id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
