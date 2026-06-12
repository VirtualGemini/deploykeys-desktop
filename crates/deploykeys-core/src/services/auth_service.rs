use crate::credentials::CredentialStore;
use crate::db::Database;
use crate::github::{GitHubClient, TokenSet};
use crate::models::{Account, AuthType};
use crate::{Error, Result};
use chrono::Utc;

/// Completes GitHub sign-ins: resolves the user, stores tokens in the system
/// keyring, and persists the account row.
pub struct AuthService {
    db: Database,
    github: GitHubClient,
}

impl AuthService {
    pub fn new(db: Database) -> Result<Self> {
        Ok(Self {
            db,
            github: GitHubClient::new()?,
        })
    }

    /// Inject a preconfigured client (tests, GitHub Enterprise).
    pub fn with_github_client(db: Database, github: GitHubClient) -> Self {
        Self { db, github }
    }

    /// Finish a device-flow sign-in.
    ///
    /// Fetches the authenticated user, stores the access (and refresh) token
    /// in the system keyring, and creates or updates the account record so the
    /// session survives restarts.
    pub async fn complete_device_flow(&self, tokens: TokenSet) -> Result<Account> {
        let user = self
            .github
            .get_authenticated_user(&tokens.access_token)
            .await?;

        let login = user.login.clone();
        let access_token = tokens.access_token.clone();
        let refresh_token = tokens.refresh_token.clone();

        // Keyring access is blocking; keep it off the async runtime threads.
        let (token_ref, refresh_token_ref) =
            tokio::task::spawn_blocking(move || -> Result<(String, Option<String>)> {
                let token_ref = CredentialStore::store_token(&login, &access_token)?;
                let refresh_token_ref = match refresh_token {
                    Some(token) => Some(CredentialStore::store_refresh_token(&login, &token)?),
                    None => None,
                };
                Ok((token_ref, refresh_token_ref))
            })
            .await
            .map_err(|e| Error::Other(format!("Keyring task failed: {}", e)))??;

        let token_expires_at = tokens
            .expires_in
            .map(|secs| Utc::now() + chrono::Duration::seconds(secs as i64));
        let now = Utc::now();

        let accounts = self.db.accounts();
        let account = match accounts.find_by_github_user_id(user.id).await? {
            Some(mut existing) => {
                existing.login = user.login.clone();
                existing.avatar_url = user.avatar_url.clone();
                existing.auth_type = AuthType::GitHubAppDeviceFlow;
                existing.token_ref = token_ref;
                existing.refresh_token_ref = refresh_token_ref;
                existing.token_expires_at = token_expires_at;
                existing.last_login_at = now;
                accounts.update(&existing).await?;
                existing
            }
            None => {
                let account = Account {
                    id: 0,
                    github_user_id: user.id,
                    login: user.login.clone(),
                    avatar_url: user.avatar_url.clone(),
                    auth_type: AuthType::GitHubAppDeviceFlow,
                    token_ref,
                    refresh_token_ref,
                    token_expires_at,
                    created_at: now,
                    last_login_at: now,
                };
                let id = accounts.create(&account).await?;
                Account { id, ..account }
            }
        };

        Ok(account)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::test_support::use_mock_keyring;
    use crate::db::test_support::test_db;
    use crate::github::TokenSet;

    /// Each test gets a distinct login: the mock keyring store is
    /// process-wide, so a shared login would race across parallel tests.
    async fn user_endpoint_mock(server: &mut mockito::ServerGuard, login: &str, id: i64) {
        server
            .mock("GET", "/user")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{"login": "{}", "id": {}, "avatar_url": "https://example.com/a.png"}}"#,
                login, id
            ))
            .create_async()
            .await;
    }

    #[tokio::test]
    async fn complete_device_flow_creates_account_and_stores_tokens() {
        use_mock_keyring();
        let (_dir, db) = test_db().await;

        let mut server = mockito::Server::new_async().await;
        user_endpoint_mock(&mut server, "octo-create", 4242).await;

        let github = GitHubClient::new().unwrap().with_base_url(server.url());
        let service = AuthService::with_github_client(db.clone(), github);

        let account = service
            .complete_device_flow(TokenSet {
                access_token: "ghu_test_access".to_string(),
                refresh_token: Some("ghr_test_refresh".to_string()),
                expires_in: Some(28800),
            })
            .await
            .unwrap();

        assert!(account.id > 0);
        assert_eq!(account.login, "octo-create");
        assert_eq!(account.github_user_id, 4242);
        assert_eq!(account.token_ref, "github_token_octo-create");
        assert_eq!(
            account.refresh_token_ref.as_deref(),
            Some("github_refresh_token_octo-create")
        );
        assert!(account.token_expires_at.is_some());

        // Tokens are retrievable through the stored references.
        let stored = CredentialStore::get_token(&account.token_ref).unwrap();
        assert_eq!(stored, "ghu_test_access");

        // The row is persisted.
        let reloaded = db.accounts().find_by_github_user_id(4242).await.unwrap();
        assert!(reloaded.is_some());
    }

    #[tokio::test]
    async fn complete_device_flow_updates_existing_account() {
        use_mock_keyring();
        let (_dir, db) = test_db().await;

        let mut server = mockito::Server::new_async().await;
        user_endpoint_mock(&mut server, "octo-update", 5151).await;

        let github = GitHubClient::new().unwrap().with_base_url(server.url());
        let service = AuthService::with_github_client(db.clone(), github);

        let first = service
            .complete_device_flow(TokenSet {
                access_token: "ghu_first".to_string(),
                refresh_token: None,
                expires_in: None,
            })
            .await
            .unwrap();

        let second = service
            .complete_device_flow(TokenSet {
                access_token: "ghu_second".to_string(),
                refresh_token: None,
                expires_in: None,
            })
            .await
            .unwrap();

        assert_eq!(first.id, second.id, "same GitHub user must reuse the row");

        let all = db.accounts().list_all().await.unwrap();
        assert_eq!(all.len(), 1);

        let stored = CredentialStore::get_token(&second.token_ref).unwrap();
        assert_eq!(stored, "ghu_second");
    }
}
