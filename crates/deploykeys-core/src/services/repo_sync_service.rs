use crate::db::Database;
use crate::github::repos::GitHubRepository;
use crate::github::GitHubClient;
use crate::models::Repository;
use crate::progress::{OperationId, ProgressReporter};
use crate::{Error, Result};

const OP_SYNC_REPOS: &str = "repos.sync";

/// Pulls the repositories an account can access (`GET /user/repos`) and persists
/// them to SQLite.
///
/// Re-running is safe: rows are matched by their GitHub id and updated in place
/// rather than duplicated.
pub struct RepoSyncService {
    db: Database,
    github: GitHubClient,
}

impl RepoSyncService {
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

    /// Sync every repository visible to the token, owned locally by `account_id`.
    /// Returns the number of repositories synced.
    pub async fn sync_repos<P: ProgressReporter>(
        &self,
        account_id: i64,
        token: &str,
        progress: &P,
    ) -> Result<usize> {
        let op = OperationId::from(OP_SYNC_REPOS);
        progress.report(op.clone(), 2);
        let repos = self.github.list_user_repos(token, &op, progress).await?;

        let total = repos.len().max(1);
        let mut count = 0;
        for repo in &repos {
            self.upsert_repository(account_id, repo).await?;
            count += 1;
            let persisted_percent = 90 + (count * 10 / total).min(10);
            progress.report(op.clone(), persisted_percent as u8);
        }
        progress.report(op, 100);
        Ok(count)
    }

    /// Create or update one repository row.
    async fn upsert_repository(&self, account_id: i64, repo: &GitHubRepository) -> Result<()> {
        let model = Repository {
            id: 0,
            github_repo_id: repo.id,
            account_id,
            owner: repo.owner.login.clone(),
            name: repo.name.clone(),
            full_name: repo.full_name.clone(),
            private: repo.private,
            archived: repo.archived,
            default_branch: repo.default_branch.clone(),
            ssh_url: repo.ssh_url.clone(),
            html_url: repo.html_url.clone(),
            language: repo.language.clone(),
            permissions_snapshot: snapshot(&repo.permissions)?,
            last_synced_at: None,
        };

        let repositories = self.db.repositories();
        match repositories.find_by_github_repo_id(repo.id).await? {
            Some(_) => repositories.update(&model).await,
            None => {
                repositories.create(&model).await?;
                Ok(())
            }
        }
    }
}

/// Serialize a permissions object to a JSON string snapshot, or `None` when the
/// field was absent (`null`).
fn snapshot(value: &serde_json::Value) -> Result<Option<String>> {
    if value.is_null() {
        return Ok(None);
    }
    serde_json::to_string(value)
        .map(Some)
        .map_err(|e| Error::Other(format!("Failed to serialize permissions: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::{seed_account, test_db};
    use crate::progress::NoOpProgressReporter;

    fn repos_body() -> &'static str {
        r#"[
            {
                "id": 1337,
                "name": "repo",
                "full_name": "octocat/repo",
                "owner": {"login": "octocat"},
                "private": true,
                "archived": false,
                "default_branch": "main",
                "ssh_url": "git@github.com:octocat/repo.git",
                "html_url": "https://github.com/octocat/repo",
                "language": "Rust",
                "permissions": {"admin": true}
            }
        ]"#
    }

    async fn mock_repos(server: &mut mockito::ServerGuard) {
        server
            .mock("GET", "/user/repos")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(repos_body())
            .expect_at_least(1)
            .create_async()
            .await;
    }

    #[tokio::test]
    async fn sync_repos_persists_repositories() {
        let (_dir, db) = test_db().await;
        let account_id = seed_account(&db).await;

        let mut server = mockito::Server::new_async().await;
        mock_repos(&mut server).await;
        let github = GitHubClient::new().unwrap().with_base_url(server.url());
        let service = RepoSyncService::with_github_client(db.clone(), github);

        let count = service
            .sync_repos(account_id, "ghp_test", &NoOpProgressReporter)
            .await
            .unwrap();
        assert_eq!(count, 1);

        let repo = db
            .repositories()
            .find_by_github_repo_id(1337)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(repo.full_name, "octocat/repo");
        assert_eq!(repo.account_id, account_id);
        assert_eq!(repo.owner, "octocat");
        assert_eq!(repo.language.as_deref(), Some("Rust"));
        assert!(repo.permissions_snapshot.is_some());
    }

    #[tokio::test]
    async fn sync_repos_is_idempotent() {
        let (_dir, db) = test_db().await;
        let account_id = seed_account(&db).await;

        let mut server = mockito::Server::new_async().await;
        mock_repos(&mut server).await;
        let github = GitHubClient::new().unwrap().with_base_url(server.url());
        let service = RepoSyncService::with_github_client(db.clone(), github);

        service
            .sync_repos(account_id, "ghp_test", &NoOpProgressReporter)
            .await
            .unwrap();
        service
            .sync_repos(account_id, "ghp_test", &NoOpProgressReporter)
            .await
            .unwrap();

        // Second run updates in place; no duplicate rows.
        assert_eq!(db.repositories().list_all().await.unwrap().len(), 1);
    }
}
