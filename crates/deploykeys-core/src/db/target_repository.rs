use crate::db::{optional_timestamp, parse_enum_field, required_timestamp};
use crate::models::Target;
use crate::{Error, Result};
use sqlx::SqlitePool;

pub struct TargetRepository {
    pool: SqlitePool,
}

/// Raw `targets` row; converted to [`Target`] in one place.
struct TargetRow {
    id: i64,
    target_type: String,
    alias: String,
    os: String,
    host: Option<String>,
    port: Option<i64>,
    username: Option<String>,
    auth_method: Option<String>,
    auth_ref: Option<String>,
    key_base_dir: String,
    status: String,
    host_key_fingerprint: Option<String>,
    created_at: i64,
    last_checked_at: Option<i64>,
}

impl TryFrom<TargetRow> for Target {
    type Error = crate::Error;

    fn try_from(r: TargetRow) -> Result<Self> {
        let port = r
            .port
            .map(|p| u16::try_from(p).map_err(|_| Error::Database(format!("Invalid port: {}", p))))
            .transpose()?;

        let auth_method = r
            .auth_method
            .map(|a| parse_enum_field(&a, "auth_method"))
            .transpose()?;

        Ok(Target {
            id: r.id,
            target_type: parse_enum_field(&r.target_type, "target_type")?,
            alias: r.alias,
            os: parse_enum_field(&r.os, "os")?,
            host: r.host,
            port,
            username: r.username,
            auth_method,
            auth_ref: r.auth_ref,
            key_base_dir: r.key_base_dir,
            status: parse_enum_field(&r.status, "status")?,
            host_key_fingerprint: r.host_key_fingerprint,
            created_at: required_timestamp(r.created_at, "created_at")?,
            last_checked_at: optional_timestamp(r.last_checked_at),
        })
    }
}

impl TargetRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, target: &Target) -> Result<i64> {
        let target_type = target.target_type.to_string();
        let os = target.os.to_string();
        let auth_method = target.auth_method.as_ref().map(|a| a.to_string());
        let status = target.status.to_string();
        let created_at = target.created_at.timestamp();
        let last_checked_at = target.last_checked_at.map(|t| t.timestamp());

        let result = sqlx::query!(
            r#"
            INSERT INTO targets (
                target_type, alias, os, host, port, username, auth_method,
                auth_ref, key_base_dir, status, host_key_fingerprint,
                created_at, last_checked_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            target_type,
            target.alias,
            os,
            target.host,
            target.port,
            target.username,
            auth_method,
            target.auth_ref,
            target.key_base_dir,
            status,
            target.host_key_fingerprint,
            created_at,
            last_checked_at
        )
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    pub async fn find_by_id(&self, id: i64) -> Result<Option<Target>> {
        let row = sqlx::query_as!(
            TargetRow,
            r#"
            SELECT id as "id!", target_type, alias, os, host, port, username, auth_method,
                   auth_ref, key_base_dir, status, host_key_fingerprint,
                   created_at, last_checked_at
            FROM targets WHERE id = ?
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(Target::try_from).transpose()
    }

    pub async fn find_by_alias(&self, alias: &str) -> Result<Option<Target>> {
        let row = sqlx::query_as!(
            TargetRow,
            r#"
            SELECT id as "id!", target_type, alias, os, host, port, username, auth_method,
                   auth_ref, key_base_dir, status, host_key_fingerprint,
                   created_at, last_checked_at
            FROM targets WHERE alias = ?
            "#,
            alias
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(Target::try_from).transpose()
    }

    pub async fn list_all(&self) -> Result<Vec<Target>> {
        let rows = sqlx::query_as!(
            TargetRow,
            r#"
            SELECT id as "id!", target_type, alias, os, host, port, username, auth_method,
                   auth_ref, key_base_dir, status, host_key_fingerprint,
                   created_at, last_checked_at
            FROM targets ORDER BY created_at ASC
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(Target::try_from).collect()
    }

    /// Update the health-related state of a target
    /// (`status`, `host_key_fingerprint`, `last_checked_at`).
    pub async fn update(&self, target: &Target) -> Result<()> {
        let status = target.status.to_string();
        let last_checked_at = target.last_checked_at.map(|t| t.timestamp());

        sqlx::query!(
            r#"
            UPDATE targets
            SET status = ?, host_key_fingerprint = ?, last_checked_at = ?
            WHERE id = ?
            "#,
            status,
            target.host_key_fingerprint,
            last_checked_at,
            target.id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_key_base_dir(&self, id: i64, key_base_dir: &str) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE targets
            SET key_base_dir = ?
            WHERE id = ?
            "#,
            key_base_dir,
            id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update editable connection fields for a remote target.
    pub async fn update_connection(&self, target: &Target) -> Result<()> {
        let os = target.os.to_string();
        let auth_method = target.auth_method.as_ref().map(|a| a.to_string());
        let status = target.status.to_string();
        let last_checked_at = target.last_checked_at.map(|t| t.timestamp());

        sqlx::query!(
            r#"
            UPDATE targets
            SET alias = ?, os = ?, host = ?, port = ?, username = ?,
                auth_method = ?, auth_ref = ?, key_base_dir = ?, status = ?,
                host_key_fingerprint = ?, last_checked_at = ?
            WHERE id = ?
            "#,
            target.alias,
            os,
            target.host,
            target.port,
            target.username,
            auth_method,
            target.auth_ref,
            target.key_base_dir,
            status,
            target.host_key_fingerprint,
            last_checked_at,
            target.id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete(&self, id: i64) -> Result<()> {
        sqlx::query!("DELETE FROM targets WHERE id = ?", id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
