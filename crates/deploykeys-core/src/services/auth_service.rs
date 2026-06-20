use crate::credentials::CredentialStore;
use crate::db::Database;
use crate::github::GitHubClient;
use crate::models::{Account, AuthType};
use crate::progress::{OperationId, ProgressReporter};
use crate::{Error, Result};
use chrono::Utc;

const OP_SIGN_IN: &str = "auth.sign_in";

/// Completes GitHub sign-ins: resolves the user, stores the token in the system
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

    /// Sign in with a Personal Access Token.
    ///
    /// Validates the token by fetching the authenticated user, stores it in the
    /// system keyring, and creates or updates the account record so the session
    /// survives restarts.
    pub async fn sign_in_with_token<P: ProgressReporter>(
        &self,
        token: String,
        progress: &P,
    ) -> Result<Account> {
        let op = OperationId::from(OP_SIGN_IN);
        progress.report(op.clone(), 10);
        let user = self.github.get_authenticated_user(&token).await?;

        let login = user.login.clone();
        progress.report(op.clone(), 35);

        // Keyring access is blocking; keep it off the async runtime threads.
        let token_ref =
            tokio::task::spawn_blocking(move || CredentialStore::store_token(&login, &token))
                .await
                .map_err(|e| Error::Other(format!("Keyring task failed: {}", e)))??;

        progress.report(op.clone(), 65);
        let now = Utc::now();
        let accounts = self.db.accounts();
        let account = match accounts.find_by_github_user_id(user.id).await? {
            Some(mut existing) => {
                existing.login = user.login.clone();
                existing.avatar_url = user.avatar_url.clone();
                existing.auth_type = AuthType::PersonalAccessToken;
                existing.token_ref = token_ref;
                existing.refresh_token_ref = None;
                existing.token_expires_at = None;
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
                    auth_type: AuthType::PersonalAccessToken,
                    token_ref,
                    refresh_token_ref: None,
                    token_expires_at: None,
                    created_at: now,
                    last_login_at: now,
                };
                let id = accounts.create(&account).await?;
                Account { id, ..account }
            }
        };

        progress.report(op, 100);
        Ok(account)
    }

    /// Sign out: remove every persisted account and its keyring tokens.
    ///
    /// The app is single-account today, but we clear all rows so no stale
    /// session can resurface on the next launch. Keyring deletion is
    /// best-effort — a missing entry must not block sign-out. Deleting the
    /// account row cascades to repositories and key bindings.
    pub async fn sign_out(&self) -> Result<()> {
        let accounts = self.db.accounts();
        for account in accounts.list_all().await? {
            let token_ref = account.token_ref.clone();
            let refresh_token_ref = account.refresh_token_ref.clone();

            // Keyring access is blocking; keep it off the async runtime threads.
            tokio::task::spawn_blocking(move || {
                let _ = CredentialStore::delete_token(&token_ref);
                if let Some(refresh_token_ref) = refresh_token_ref {
                    let _ = CredentialStore::delete_credential(&refresh_token_ref);
                }
            })
            .await
            .map_err(|e| Error::Other(format!("Keyring task failed: {}", e)))?;

            accounts.delete(account.id).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::test_support::use_mock_keyring;
    use crate::db::test_support::test_db;
    use crate::progress::NoOpProgressReporter;

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
    async fn sign_in_creates_account_and_stores_token() {
        use_mock_keyring();
        let (_dir, db) = test_db().await;

        let mut server = mockito::Server::new_async().await;
        user_endpoint_mock(&mut server, "octo-create", 4242).await;

        let github = GitHubClient::new().unwrap().with_base_url(server.url());
        let service = AuthService::with_github_client(db.clone(), github);

        let account = service
            .sign_in_with_token("ghp_test_pat".to_string(), &NoOpProgressReporter)
            .await
            .unwrap();

        assert!(account.id > 0);
        assert_eq!(account.login, "octo-create");
        assert_eq!(account.github_user_id, 4242);
        assert!(account.token_ref.starts_with("github_token_octo-create_"));
        assert_eq!(account.auth_type, AuthType::PersonalAccessToken);

        // Token is retrievable through the stored reference.
        let stored = CredentialStore::get_token(&account.token_ref).unwrap();
        assert_eq!(stored, "ghp_test_pat");

        // The row is persisted.
        let reloaded = db.accounts().find_by_github_user_id(4242).await.unwrap();
        assert!(reloaded.is_some());
    }

    #[tokio::test]
    async fn sign_in_updates_existing_account() {
        use_mock_keyring();
        let (_dir, db) = test_db().await;

        let mut server = mockito::Server::new_async().await;
        user_endpoint_mock(&mut server, "octo-update", 5151).await;

        let github = GitHubClient::new().unwrap().with_base_url(server.url());
        let service = AuthService::with_github_client(db.clone(), github);

        let first = service
            .sign_in_with_token("ghp_first".to_string(), &NoOpProgressReporter)
            .await
            .unwrap();

        let second = service
            .sign_in_with_token("ghp_second".to_string(), &NoOpProgressReporter)
            .await
            .unwrap();

        assert_eq!(first.id, second.id, "same GitHub user must reuse the row");

        let all = db.accounts().list_all().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_ne!(
            first.token_ref, second.token_ref,
            "re-login must create a new keychain item instead of updating the old one"
        );

        let stored = CredentialStore::get_token(&second.token_ref).unwrap();
        assert_eq!(stored, "ghp_second");
    }

    #[tokio::test]
    async fn sign_out_clears_account_and_token() {
        use_mock_keyring();
        let (_dir, db) = test_db().await;

        let mut server = mockito::Server::new_async().await;
        user_endpoint_mock(&mut server, "octo-signout", 6262).await;

        let github = GitHubClient::new().unwrap().with_base_url(server.url());
        let service = AuthService::with_github_client(db.clone(), github);

        let account = service
            .sign_in_with_token("ghp_signout".to_string(), &NoOpProgressReporter)
            .await
            .unwrap();

        service.sign_out().await.unwrap();

        // The session is gone: no rows, and the token is no longer retrievable.
        assert!(db.accounts().list_all().await.unwrap().is_empty());
        assert!(CredentialStore::get_token(&account.token_ref).is_err());
    }

    #[tokio::test]
    async fn sign_out_is_ok_with_no_session() {
        use_mock_keyring();
        let (_dir, db) = test_db().await;
        let github = GitHubClient::new().unwrap();
        let service = AuthService::with_github_client(db, github);

        service.sign_out().await.unwrap();
    }
}
