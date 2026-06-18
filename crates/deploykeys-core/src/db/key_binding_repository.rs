use crate::db::{optional_timestamp, parse_enum_field, required_timestamp};
use crate::models::{KeyBinding, KeyBindingStatus};
use crate::Result;
use chrono::Utc;
use sqlx::SqlitePool;

pub struct KeyBindingRepository {
    pool: SqlitePool,
}

/// Raw `key_bindings` row; converted to [`KeyBinding`] in one place.
struct KeyBindingRow {
    id: i64,
    repo_id: i64,
    target_id: i64,
    github_deploy_key_id: Option<i64>,
    deploy_key_title: String,
    algorithm: String,
    permission: String,
    public_key: String,
    public_key_fingerprint: String,
    private_key_path: String,
    private_key_residency: String,
    status: String,
    created_at: i64,
    last_verified_at: Option<i64>,
}

impl TryFrom<KeyBindingRow> for KeyBinding {
    type Error = crate::Error;

    fn try_from(r: KeyBindingRow) -> Result<Self> {
        Ok(KeyBinding {
            id: r.id,
            repo_id: r.repo_id,
            target_id: r.target_id,
            github_deploy_key_id: r.github_deploy_key_id,
            deploy_key_title: r.deploy_key_title,
            algorithm: parse_enum_field(&r.algorithm, "algorithm")?,
            permission: parse_enum_field(&r.permission, "permission")?,
            public_key: r.public_key,
            public_key_fingerprint: r.public_key_fingerprint,
            private_key_path: r.private_key_path,
            private_key_residency: parse_enum_field(
                &r.private_key_residency,
                "private_key_residency",
            )?,
            status: parse_enum_field(&r.status, "status")?,
            created_at: required_timestamp(r.created_at, "created_at")?,
            last_verified_at: optional_timestamp(r.last_verified_at),
        })
    }
}

impl KeyBindingRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, binding: &KeyBinding) -> Result<i64> {
        let algorithm = binding.algorithm.to_string();
        let permission = binding.permission.to_string();
        let residency = binding.private_key_residency.to_string();
        let status = binding.status.to_string();
        let created_at = binding.created_at.timestamp();
        let last_verified_at = binding.last_verified_at.map(|t| t.timestamp());

        let result = sqlx::query!(
            r#"
            INSERT INTO key_bindings (
                repo_id, target_id, github_deploy_key_id, deploy_key_title,
                algorithm, permission, public_key, public_key_fingerprint,
                private_key_path, private_key_residency, status,
                created_at, last_verified_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            binding.repo_id,
            binding.target_id,
            binding.github_deploy_key_id,
            binding.deploy_key_title,
            algorithm,
            permission,
            binding.public_key,
            binding.public_key_fingerprint,
            binding.private_key_path,
            residency,
            status,
            created_at,
            last_verified_at
        )
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    pub async fn find_by_id(&self, id: i64) -> Result<Option<KeyBinding>> {
        let row = sqlx::query_as!(
            KeyBindingRow,
            r#"
            SELECT id as "id!", repo_id, target_id, github_deploy_key_id, deploy_key_title,
                   algorithm, permission, public_key, public_key_fingerprint,
                   private_key_path, private_key_residency, status, created_at, last_verified_at
            FROM key_bindings WHERE id = ?
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(KeyBinding::try_from).transpose()
    }

    pub async fn find_by_repo_and_target(
        &self,
        repo_id: i64,
        target_id: i64,
    ) -> Result<Option<KeyBinding>> {
        let row = sqlx::query_as!(
            KeyBindingRow,
            r#"
            SELECT id as "id!", repo_id, target_id, github_deploy_key_id, deploy_key_title,
                   algorithm, permission, public_key, public_key_fingerprint,
                   private_key_path, private_key_residency, status, created_at, last_verified_at
            FROM key_bindings WHERE repo_id = ? AND target_id = ?
            "#,
            repo_id,
            target_id
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(KeyBinding::try_from).transpose()
    }

    pub async fn list_by_repo(&self, repo_id: i64) -> Result<Vec<KeyBinding>> {
        let rows = sqlx::query_as!(
            KeyBindingRow,
            r#"
            SELECT id as "id!", repo_id, target_id, github_deploy_key_id, deploy_key_title,
                   algorithm, permission, public_key, public_key_fingerprint,
                   private_key_path, private_key_residency, status, created_at, last_verified_at
            FROM key_bindings WHERE repo_id = ? ORDER BY created_at DESC
            "#,
            repo_id
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(KeyBinding::try_from).collect()
    }

    pub async fn list_by_target(&self, target_id: i64) -> Result<Vec<KeyBinding>> {
        let rows = sqlx::query_as!(
            KeyBindingRow,
            r#"
            SELECT id as "id!", repo_id, target_id, github_deploy_key_id, deploy_key_title,
                   algorithm, permission, public_key, public_key_fingerprint,
                   private_key_path, private_key_residency, status, created_at, last_verified_at
            FROM key_bindings WHERE target_id = ? ORDER BY created_at DESC
            "#,
            target_id
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(KeyBinding::try_from).collect()
    }

    pub async fn list_all(&self) -> Result<Vec<KeyBinding>> {
        let rows = sqlx::query_as!(
            KeyBindingRow,
            r#"
            SELECT id as "id!", repo_id, target_id, github_deploy_key_id, deploy_key_title,
                   algorithm, permission, public_key, public_key_fingerprint,
                   private_key_path, private_key_residency, status, created_at, last_verified_at
            FROM key_bindings ORDER BY created_at DESC
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(KeyBinding::try_from).collect()
    }

    /// Set the binding status and stamp `last_verified_at` with the current time.
    pub async fn update_status(&self, id: i64, status: KeyBindingStatus) -> Result<()> {
        let status_str = status.to_string();
        let now = Utc::now().timestamp();

        sqlx::query!(
            r#"
            UPDATE key_bindings
            SET status = ?, last_verified_at = ?
            WHERE id = ?
            "#,
            status_str,
            now,
            id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update the GitHub-facing state of a binding
    /// (`github_deploy_key_id`, `status`, `last_verified_at`).
    pub async fn update(&self, binding: &KeyBinding) -> Result<()> {
        let status = binding.status.to_string();
        let last_verified_at = binding.last_verified_at.map(|t| t.timestamp());

        sqlx::query!(
            r#"
            UPDATE key_bindings
            SET github_deploy_key_id = ?, status = ?, last_verified_at = ?
            WHERE id = ?
            "#,
            binding.github_deploy_key_id,
            status,
            last_verified_at,
            binding.id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Replace a stale binding row with a newly uploaded deploy key while
    /// preserving the row id used by the UI.
    pub async fn replace(&self, binding: &KeyBinding) -> Result<()> {
        let algorithm = binding.algorithm.to_string();
        let permission = binding.permission.to_string();
        let residency = binding.private_key_residency.to_string();
        let status = binding.status.to_string();
        let created_at = binding.created_at.timestamp();
        let last_verified_at = binding.last_verified_at.map(|t| t.timestamp());

        sqlx::query!(
            r#"
            UPDATE key_bindings
            SET repo_id = ?, target_id = ?, github_deploy_key_id = ?, deploy_key_title = ?,
                algorithm = ?, permission = ?, public_key = ?, public_key_fingerprint = ?,
                private_key_path = ?, private_key_residency = ?, status = ?,
                created_at = ?, last_verified_at = ?
            WHERE id = ?
            "#,
            binding.repo_id,
            binding.target_id,
            binding.github_deploy_key_id,
            binding.deploy_key_title,
            algorithm,
            permission,
            binding.public_key,
            binding.public_key_fingerprint,
            binding.private_key_path,
            residency,
            status,
            created_at,
            last_verified_at,
            binding.id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete(&self, id: i64) -> Result<()> {
        sqlx::query!("DELETE FROM key_bindings WHERE id = ?", id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
