use crate::{Error, Result};
use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;

pub mod account_repository;
pub mod key_binding_repository;
pub mod repository_repository;
pub mod ssh_key_repository;
pub mod target_repository;

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod tests;

pub use account_repository::AccountRepository;
pub use key_binding_repository::KeyBindingRepository;
pub use repository_repository::RepositoryRepository;
pub use ssh_key_repository::SshKeyRepository;
pub use target_repository::TargetRepository;

const MAX_CONNECTIONS: u32 = 5;

/// SQLite-backed application database.
#[derive(Clone, Debug)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Open (or create) the database at `db_path`.
    ///
    /// Enables WAL journaling and foreign key enforcement. Call
    /// [`Database::run_migrations`] before using any repository.
    pub async fn new(db_path: &Path) -> Result<Self> {
        if db_path.as_os_str().is_empty() {
            return Err(Error::Validation(
                "Database path cannot be empty".to_string(),
            ));
        }

        let options = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(MAX_CONNECTIONS)
            .connect_with(options)
            .await?;

        Ok(Self { pool })
    }

    /// Apply all pending migrations embedded from `migrations/`.
    pub async fn run_migrations(&self) -> Result<()> {
        sqlx::migrate!("../../migrations")
            .run(&self.pool)
            .await
            .map_err(|e| Error::Database(format!("Migration failed: {}", e)))?;
        self.ensure_ssh_key_directory_index_scope().await?;
        Ok(())
    }

    async fn ensure_ssh_key_directory_index_scope(&self) -> Result<()> {
        sqlx::query!("DROP INDEX IF EXISTS idx_ssh_keys_directory")
            .execute(&self.pool)
            .await?;
        sqlx::query!(
            r#"
            CREATE UNIQUE INDEX IF NOT EXISTS idx_ssh_keys_target_directory
            ON ssh_keys(target_id, directory)
            "#
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get account repository
    pub fn accounts(&self) -> AccountRepository {
        AccountRepository::new(self.pool.clone())
    }

    /// Get repositories repository
    pub fn repositories(&self) -> RepositoryRepository {
        RepositoryRepository::new(self.pool.clone())
    }

    /// Get targets repository
    pub fn targets(&self) -> TargetRepository {
        TargetRepository::new(self.pool.clone())
    }

    /// Get key bindings repository
    pub fn key_bindings(&self) -> KeyBindingRepository {
        KeyBindingRepository::new(self.pool.clone())
    }

    /// Get SSH keys repository
    pub fn ssh_keys(&self) -> SshKeyRepository {
        SshKeyRepository::new(self.pool.clone())
    }

    /// Read an application setting, or `None` if it has never been set.
    pub async fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let row = sqlx::query!("SELECT value FROM app_settings WHERE key = ?", key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.value))
    }

    /// Insert or update an application setting.
    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO app_settings (key, value) VALUES (?, ?)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            "#,
            key,
            value
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

/// Parse a TEXT column into an enum, mapping failures to [`Error::Database`].
pub(crate) fn parse_enum_field<T>(value: &str, field: &str) -> Result<T>
where
    T: FromStr<Err = String>,
{
    value
        .parse()
        .map_err(|e| Error::Database(format!("Invalid {}: {}", field, e)))
}

/// Convert a required unix-seconds column into a `DateTime<Utc>`.
pub(crate) fn required_timestamp(secs: i64, field: &str) -> Result<DateTime<Utc>> {
    DateTime::from_timestamp(secs, 0)
        .ok_or_else(|| Error::Database(format!("Invalid {} timestamp: {}", field, secs)))
}

/// Convert an optional unix-seconds column into an optional `DateTime<Utc>`.
pub(crate) fn optional_timestamp(secs: Option<i64>) -> Option<DateTime<Utc>> {
    secs.and_then(|t| DateTime::from_timestamp(t, 0))
}

#[cfg(test)]
mod integration_tests;
