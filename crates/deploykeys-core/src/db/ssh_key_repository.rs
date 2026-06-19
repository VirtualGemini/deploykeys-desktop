use crate::db::{parse_enum_field, required_timestamp};
use crate::models::SshKey;
use crate::Result;
use sqlx::SqlitePool;

pub struct SshKeyRepository {
    pool: SqlitePool,
}

/// Raw `ssh_keys` row; converted to [`SshKey`] in one place.
struct SshKeyRow {
    id: i64,
    directory: String,
    algorithm: String,
    public_key: String,
    public_key_fingerprint: String,
    private_key_path: String,
    public_key_path: String,
    comment: String,
    remark: String,
    target_id: i64,
    created_at: i64,
}

impl TryFrom<SshKeyRow> for SshKey {
    type Error = crate::Error;

    fn try_from(r: SshKeyRow) -> Result<Self> {
        Ok(SshKey {
            id: r.id,
            directory: r.directory,
            algorithm: parse_enum_field(&r.algorithm, "algorithm")?,
            public_key: r.public_key,
            public_key_fingerprint: r.public_key_fingerprint,
            private_key_path: r.private_key_path,
            public_key_path: r.public_key_path,
            comment: r.comment,
            remark: r.remark,
            target_id: r.target_id,
            created_at: required_timestamp(r.created_at, "created_at")?,
        })
    }
}

impl SshKeyRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, key: &SshKey) -> Result<i64> {
        let algorithm = key.algorithm.to_string();
        let created_at = key.created_at.timestamp();

        let result = sqlx::query!(
            r#"
            INSERT INTO ssh_keys (
                directory, algorithm, public_key, public_key_fingerprint,
                private_key_path, public_key_path, comment, remark, target_id, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            key.directory,
            algorithm,
            key.public_key,
            key.public_key_fingerprint,
            key.private_key_path,
            key.public_key_path,
            key.comment,
            key.remark,
            key.target_id,
            created_at
        )
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    pub async fn find_by_id(&self, id: i64) -> Result<Option<SshKey>> {
        let row = sqlx::query_as!(
            SshKeyRow,
            r#"
            SELECT id as "id!", directory, algorithm, public_key, public_key_fingerprint,
                   private_key_path, public_key_path, comment, remark, target_id, created_at
            FROM ssh_keys WHERE id = ?
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(SshKey::try_from).transpose()
    }

    pub async fn find_by_directory(&self, directory: &str) -> Result<Option<SshKey>> {
        let row = sqlx::query_as!(
            SshKeyRow,
            r#"
            SELECT id as "id!", directory, algorithm, public_key, public_key_fingerprint,
                   private_key_path, public_key_path, comment, remark, target_id, created_at
            FROM ssh_keys WHERE directory = ?
            "#,
            directory
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(SshKey::try_from).transpose()
    }

    pub async fn find_by_directory_and_target(
        &self,
        directory: &str,
        target_id: i64,
    ) -> Result<Option<SshKey>> {
        let row = sqlx::query_as!(
            SshKeyRow,
            r#"
            SELECT id as "id!", directory, algorithm, public_key, public_key_fingerprint,
                   private_key_path, public_key_path, comment, remark, target_id, created_at
            FROM ssh_keys WHERE directory = ? AND target_id = ?
            "#,
            directory,
            target_id
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(SshKey::try_from).transpose()
    }

    pub async fn list_by_target(&self, target_id: i64) -> Result<Vec<SshKey>> {
        let rows = sqlx::query_as!(
            SshKeyRow,
            r#"
            SELECT id as "id!", directory, algorithm, public_key, public_key_fingerprint,
                   private_key_path, public_key_path, comment, remark, target_id, created_at
            FROM ssh_keys WHERE target_id = ? ORDER BY created_at DESC
            "#,
            target_id
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(SshKey::try_from).collect()
    }

    pub async fn list_all(&self) -> Result<Vec<SshKey>> {
        let rows = sqlx::query_as!(
            SshKeyRow,
            r#"
            SELECT id as "id!", directory, algorithm, public_key, public_key_fingerprint,
                   private_key_path, public_key_path, comment, remark, target_id, created_at
            FROM ssh_keys ORDER BY created_at DESC
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(SshKey::try_from).collect()
    }

    /// Update the editable fields of a key: the directory and the free-form
    /// remark. The private/public key paths move with the directory, so they are
    /// written alongside it. The `comment` is intentionally not editable here —
    /// it is embedded in the on-disk key file and immutable after creation.
    pub async fn update_key(
        &self,
        id: i64,
        directory: &str,
        remark: &str,
        private_key_path: &str,
        public_key_path: &str,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE ssh_keys
            SET directory = ?, remark = ?, private_key_path = ?, public_key_path = ?
            WHERE id = ?
            "#,
            directory,
            remark,
            private_key_path,
            public_key_path,
            id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete(&self, id: i64) -> Result<()> {
        sqlx::query!("DELETE FROM ssh_keys WHERE id = ?", id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
