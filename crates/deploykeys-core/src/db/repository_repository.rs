use crate::db::optional_timestamp;
use crate::models::Repository;
use crate::Result;
use chrono::Utc;
use sqlx::SqlitePool;

pub struct RepositoryRepository {
    pool: SqlitePool,
}

/// Raw `repositories` row; converted to [`Repository`] in one place.
struct RepositoryRow {
    id: i64,
    github_repo_id: i64,
    installation_id: i64,
    owner: String,
    name: String,
    full_name: String,
    private: bool,
    archived: bool,
    default_branch: Option<String>,
    ssh_url: String,
    html_url: String,
    permissions_snapshot: Option<String>,
    last_synced_at: Option<i64>,
}

impl TryFrom<RepositoryRow> for Repository {
    type Error = crate::Error;

    fn try_from(r: RepositoryRow) -> Result<Self> {
        Ok(Repository {
            id: r.id,
            github_repo_id: r.github_repo_id,
            installation_id: r.installation_id,
            owner: r.owner,
            name: r.name,
            full_name: r.full_name,
            private: r.private,
            archived: r.archived,
            default_branch: r.default_branch,
            ssh_url: r.ssh_url,
            html_url: r.html_url,
            permissions_snapshot: r.permissions_snapshot,
            last_synced_at: optional_timestamp(r.last_synced_at),
        })
    }
}

impl RepositoryRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, repo: &Repository) -> Result<i64> {
        let private = repo.private as i64;
        let archived = repo.archived as i64;
        let last_synced_at = repo.last_synced_at.map(|t| t.timestamp());

        let result = sqlx::query!(
            r#"
            INSERT INTO repositories (
                github_repo_id, installation_id, owner, name, full_name,
                private, archived, default_branch, ssh_url, html_url,
                permissions_snapshot, last_synced_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            repo.github_repo_id,
            repo.installation_id,
            repo.owner,
            repo.name,
            repo.full_name,
            private,
            archived,
            repo.default_branch,
            repo.ssh_url,
            repo.html_url,
            repo.permissions_snapshot,
            last_synced_at
        )
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    pub async fn find_by_id(&self, id: i64) -> Result<Option<Repository>> {
        let row = sqlx::query_as!(
            RepositoryRow,
            r#"
            SELECT id as "id!", github_repo_id, installation_id, owner, name, full_name,
                   private as "private: bool", archived as "archived: bool",
                   default_branch, ssh_url, html_url, permissions_snapshot, last_synced_at
            FROM repositories WHERE id = ?
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(Repository::try_from).transpose()
    }

    pub async fn find_by_github_repo_id(&self, github_repo_id: i64) -> Result<Option<Repository>> {
        let row = sqlx::query_as!(
            RepositoryRow,
            r#"
            SELECT id as "id!", github_repo_id, installation_id, owner, name, full_name,
                   private as "private: bool", archived as "archived: bool",
                   default_branch, ssh_url, html_url, permissions_snapshot, last_synced_at
            FROM repositories WHERE github_repo_id = ?
            "#,
            github_repo_id
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(Repository::try_from).transpose()
    }

    pub async fn list_by_installation(&self, installation_id: i64) -> Result<Vec<Repository>> {
        let rows = sqlx::query_as!(
            RepositoryRow,
            r#"
            SELECT id as "id!", github_repo_id, installation_id, owner, name, full_name,
                   private as "private: bool", archived as "archived: bool",
                   default_branch, ssh_url, html_url, permissions_snapshot, last_synced_at
            FROM repositories WHERE installation_id = ? ORDER BY full_name ASC
            "#,
            installation_id
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(Repository::try_from).collect()
    }

    pub async fn list_all(&self) -> Result<Vec<Repository>> {
        let rows = sqlx::query_as!(
            RepositoryRow,
            r#"
            SELECT id as "id!", github_repo_id, installation_id, owner, name, full_name,
                   private as "private: bool", archived as "archived: bool",
                   default_branch, ssh_url, html_url, permissions_snapshot, last_synced_at
            FROM repositories ORDER BY full_name ASC
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(Repository::try_from).collect()
    }

    pub async fn update_sync_time(&self, id: i64) -> Result<()> {
        let now = Utc::now().timestamp();

        sqlx::query!(
            "UPDATE repositories SET last_synced_at = ? WHERE id = ?",
            now,
            id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete(&self, id: i64) -> Result<()> {
        sqlx::query!("DELETE FROM repositories WHERE id = ?", id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
