use crate::db::{optional_timestamp, parse_enum_field, required_timestamp};
use crate::models::Account;
use crate::Result;
use sqlx::SqlitePool;

pub struct AccountRepository {
    pool: SqlitePool,
}

/// Raw `accounts` row; converted to [`Account`] in one place via `TryFrom`.
struct AccountRow {
    id: i64,
    github_user_id: i64,
    login: String,
    avatar_url: Option<String>,
    auth_type: String,
    token_ref: String,
    refresh_token_ref: Option<String>,
    token_expires_at: Option<i64>,
    created_at: i64,
    last_login_at: i64,
}

impl TryFrom<AccountRow> for Account {
    type Error = crate::Error;

    fn try_from(r: AccountRow) -> Result<Self> {
        Ok(Account {
            id: r.id,
            github_user_id: r.github_user_id,
            login: r.login,
            avatar_url: r.avatar_url,
            auth_type: parse_enum_field(&r.auth_type, "auth_type")?,
            token_ref: r.token_ref,
            refresh_token_ref: r.refresh_token_ref,
            token_expires_at: optional_timestamp(r.token_expires_at),
            created_at: required_timestamp(r.created_at, "created_at")?,
            last_login_at: required_timestamp(r.last_login_at, "last_login_at")?,
        })
    }
}

impl AccountRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, account: &Account) -> Result<i64> {
        let auth_type = account.auth_type.to_string();
        let created_at = account.created_at.timestamp();
        let last_login_at = account.last_login_at.timestamp();
        let token_expires_at = account.token_expires_at.map(|t| t.timestamp());

        let result = sqlx::query!(
            r#"
            INSERT INTO accounts (
                github_user_id, login, avatar_url, auth_type, token_ref,
                refresh_token_ref, token_expires_at, created_at, last_login_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            account.github_user_id,
            account.login,
            account.avatar_url,
            auth_type,
            account.token_ref,
            account.refresh_token_ref,
            token_expires_at,
            created_at,
            last_login_at
        )
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    pub async fn find_by_id(&self, id: i64) -> Result<Option<Account>> {
        let row = sqlx::query_as!(
            AccountRow,
            r#"
            SELECT id as "id!", github_user_id, login, avatar_url, auth_type, token_ref,
                   refresh_token_ref, token_expires_at, created_at, last_login_at
            FROM accounts WHERE id = ?
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(Account::try_from).transpose()
    }

    pub async fn find_by_github_user_id(&self, github_user_id: i64) -> Result<Option<Account>> {
        let row = sqlx::query_as!(
            AccountRow,
            r#"
            SELECT id as "id!", github_user_id, login, avatar_url, auth_type, token_ref,
                   refresh_token_ref, token_expires_at, created_at, last_login_at
            FROM accounts WHERE github_user_id = ?
            "#,
            github_user_id
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(Account::try_from).transpose()
    }

    pub async fn update(&self, account: &Account) -> Result<()> {
        let auth_type = account.auth_type.to_string();
        let last_login_at = account.last_login_at.timestamp();
        let token_expires_at = account.token_expires_at.map(|t| t.timestamp());

        sqlx::query!(
            r#"
            UPDATE accounts
            SET login = ?, avatar_url = ?, auth_type = ?, token_ref = ?,
                refresh_token_ref = ?, token_expires_at = ?, last_login_at = ?
            WHERE id = ?
            "#,
            account.login,
            account.avatar_url,
            auth_type,
            account.token_ref,
            account.refresh_token_ref,
            token_expires_at,
            last_login_at,
            account.id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn list_all(&self) -> Result<Vec<Account>> {
        let rows = sqlx::query_as!(
            AccountRow,
            r#"
            SELECT id as "id!", github_user_id, login, avatar_url, auth_type, token_ref,
                   refresh_token_ref, token_expires_at, created_at, last_login_at
            FROM accounts ORDER BY last_login_at DESC
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(Account::try_from).collect()
    }

    pub async fn delete(&self, id: i64) -> Result<()> {
        sqlx::query!("DELETE FROM accounts WHERE id = ?", id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
