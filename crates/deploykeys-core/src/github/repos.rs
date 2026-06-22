use crate::progress::{OperationId, ProgressReporter};
use crate::Result;
use reqwest::Method;
use serde::Deserialize;

use super::GitHubClient;

/// Page size for the paginated repos endpoint (GitHub's maximum is 100).
const PER_PAGE: u32 = 100;

/// Hard cap on pages fetched, guarding against a server that never reports a
/// shrinking page. 100 pages * 100 = 10k repos.
const MAX_PAGES: u32 = 100;

/// The owner object on a repository.
#[derive(Debug, Clone, Deserialize)]
pub struct RepoOwner {
    pub login: String,
}

/// A repository as returned by `GET /user/repos` and
/// `GET /installation/repositories`. Both endpoints use the same repo shape.
#[derive(Debug, Clone, Deserialize)]
pub struct GitHubRepository {
    pub id: i64,
    pub name: String,
    pub full_name: String,
    pub owner: RepoOwner,
    pub private: bool,
    pub archived: bool,
    pub default_branch: Option<String>,
    pub ssh_url: String,
    pub html_url: String,
    pub language: Option<String>,
    #[serde(default)]
    pub permissions: serde_json::Value,
}

/// The `{"total_count": N, "repositories": [...]}` wrapper returned by
/// `GET /installation/repositories`. Only the inner array is needed.
#[derive(Debug, Deserialize)]
struct InstallationRepositoriesResponse {
    #[serde(default)]
    repositories: Vec<GitHubRepository>,
}

/// Fine-grained PATs (`github_pat_...`) are installation tokens: the user's
/// own repos are invisible to them via `/user/repos`, which returns only the
/// personal-account repos. They must be listed through the installation
/// endpoint instead. Classic PATs (`ghp_...`) and OAuth tokens are user tokens
/// and keep using `/user/repos`.
fn is_fine_grained_pat(token: &str) -> bool {
    token.starts_with("github_pat_")
}

impl GitHubClient {
    /// List every repository the token can access, across all pages. Progress
    /// is reported as pages are fetched and, when available, as local
    /// persistence proceeds.
    ///
    /// Fine-grained PATs are routed to `GET /installation/repositories` (their
    /// repos are not exposed via `/user/repos`); classic PATs and OAuth tokens
    /// keep using `GET /user/repos`.
    pub async fn list_user_repos<P: ProgressReporter>(
        &self,
        token: &str,
        operation: &OperationId,
        progress: &P,
    ) -> Result<Vec<GitHubRepository>> {
        let mut all = Vec::new();
        progress.report(operation.clone(), 5);
        for page in 1..=MAX_PAGES {
            let path = format!(
                "{}?per_page={}&page={}",
                endpoint_path(token),
                PER_PAGE,
                page
            );
            let repos = if is_fine_grained_pat(token) {
                // `/installation/repositories` wraps the list in an object.
                let resp: InstallationRepositoriesResponse =
                    self.request(token, Method::GET, &path).await?;
                resp.repositories
            } else {
                // `/user/repos` returns a bare array (no wrapper).
                self.request::<Vec<GitHubRepository>>(token, Method::GET, &path)
                    .await?
            };

            let page_len = repos.len();
            all.extend(repos);

            progress.report(
                operation.clone(),
                percent_for_page(page, page_len < PER_PAGE as usize),
            );

            if page_len < PER_PAGE as usize {
                break;
            }
        }
        Ok(all)
    }
}

/// Pick the listing endpoint based on the token type.
fn endpoint_path(token: &str) -> &'static str {
    if is_fine_grained_pat(token) {
        "/installation/repositories"
    } else {
        "/user/repos"
    }
}

fn percent_for_page(page: u32, is_last: bool) -> u8 {
    if is_last {
        return 90;
    }
    // Fetched pages are the dominant cost. Cap at 85 until persistence finishes.
    let base = 10 + (page.saturating_sub(1)) * 2;
    base.min(85) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::NoOpProgressReporter;

    fn client_for(server: &mockito::Server) -> GitHubClient {
        GitHubClient::new()
            .expect("client builds")
            .with_base_url(server.url())
    }

    fn repo_json(id: i64, full_name: &str) -> String {
        let owner = full_name.split('/').next().unwrap_or("owner");
        format!(
            r#"{{
                "id": {id},
                "name": "repo",
                "full_name": "{full_name}",
                "owner": {{"login": "{owner}"}},
                "private": true,
                "archived": false,
                "default_branch": "main",
                "ssh_url": "git@github.com:{full_name}.git",
                "html_url": "https://github.com/{full_name}",
                "language": "Rust",
                "permissions": {{"admin": true}}
            }}"#
        )
    }

    #[tokio::test]
    async fn list_user_repos_parses_single_page() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/user/repos")
            .match_query(mockito::Matcher::Any)
            .match_header("authorization", "Bearer token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!("[{}]", repo_json(1, "octocat/hello")))
            .create_async()
            .await;

        let repos = client_for(&server)
            .list_user_repos("token", &OperationId::from("test"), &NoOpProgressReporter)
            .await
            .unwrap();

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].full_name, "octocat/hello");
        assert_eq!(repos[0].owner.login, "octocat");
        assert_eq!(repos[0].language.as_deref(), Some("Rust"));
    }

    #[tokio::test]
    async fn list_user_repos_accumulates_pages() {
        let mut server = mockito::Server::new_async().await;

        let first_page: Vec<String> = (0..PER_PAGE)
            .map(|i| repo_json(i as i64, &format!("o/repo{}", i)))
            .collect();
        server
            .mock("GET", "/user/repos")
            .match_query(mockito::Matcher::UrlEncoded("page".into(), "1".into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!("[{}]", first_page.join(",")))
            .create_async()
            .await;
        server
            .mock("GET", "/user/repos")
            .match_query(mockito::Matcher::UrlEncoded("page".into(), "2".into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!("[{}]", repo_json(100, "o/repo100")))
            .create_async()
            .await;

        let repos = client_for(&server)
            .list_user_repos("token", &OperationId::from("test"), &NoOpProgressReporter)
            .await
            .unwrap();

        assert_eq!(repos.len(), 101);
        assert_eq!(repos[100].full_name, "o/repo100");
    }

    #[tokio::test]
    async fn error_status_becomes_github_error() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/user/repos")
            .match_query(mockito::Matcher::Any)
            .with_status(401)
            .with_header("content-type", "application/json")
            .with_body(r#"{"message": "Bad credentials"}"#)
            .create_async()
            .await;

        let error = client_for(&server)
            .list_user_repos("bad", &OperationId::from("test"), &NoOpProgressReporter)
            .await
            .unwrap_err();

        assert!(matches!(error, crate::Error::GitHub(_)));
        assert!(error.to_string().contains("401"));
    }

    // ---- fine-grained PAT routing -------------------------------------------

    #[test]
    fn fine_grained_pat_detected_by_prefix() {
        // GitHub always mints these with the lowercase `github_pat_` prefix.
        assert!(is_fine_grained_pat("github_pat_11ABCDE0123456789"));
        assert!(!is_fine_grained_pat("ghp_abcdefghijklmnopqrstuvwxyz"));
        assert!(!is_fine_grained_pat("gho_abcdefghijklmnopqrstuvwxyz"));
        assert!(!is_fine_grained_pat("token"));
    }

    #[test]
    fn endpoint_path_routes_by_token_type() {
        assert_eq!(
            endpoint_path("github_pat_11ABCDE0123456789"),
            "/installation/repositories"
        );
        assert_eq!(endpoint_path("ghp_token"), "/user/repos");
        assert_eq!(endpoint_path("gho_token"), "/user/repos");
        assert_eq!(endpoint_path("anything_else"), "/user/repos");
    }

    #[tokio::test]
    async fn fine_grained_pat_uses_installation_endpoint() {
        let mut server = mockito::Server::new_async().await;
        // A fine-grained PAT must hit `/installation/repositories`, NOT
        // `/user/repos`. Mock only the installation path so a stray
        // `/user/repos` call would fail with a 404 -> error.
        server
            .mock("GET", "/installation/repositories")
            .match_query(mockito::Matcher::Any)
            .match_header("authorization", "Bearer github_pat_11ABCDEF")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{"total_count": 1, "repositories": [{}]}}"#,
                repo_json(42, "myorg/private-repo")
            ))
            .create_async()
            .await;

        let repos = client_for(&server)
            .list_user_repos(
                "github_pat_11ABCDEF",
                &OperationId::from("test"),
                &NoOpProgressReporter,
            )
            .await
            .unwrap();

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].full_name, "myorg/private-repo");
        assert_eq!(repos[0].owner.login, "myorg");
        assert!(repos[0].private);
    }

    #[tokio::test]
    async fn fine_grained_pat_paginates_installation_endpoint() {
        let mut server = mockito::Server::new_async().await;

        let first_page: Vec<String> = (0..PER_PAGE)
            .map(|i| repo_json(i as i64, &format!("org/repo{}", i)))
            .collect();
        server
            .mock("GET", "/installation/repositories")
            .match_query(mockito::Matcher::UrlEncoded("page".into(), "1".into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{"total_count": 101, "repositories": [{}]}}"#,
                first_page.join(",")
            ))
            .create_async()
            .await;
        server
            .mock("GET", "/installation/repositories")
            .match_query(mockito::Matcher::UrlEncoded("page".into(), "2".into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{"total_count": 101, "repositories": [{}]}}"#,
                repo_json(100, "org/repo100")
            ))
            .create_async()
            .await;

        let repos = client_for(&server)
            .list_user_repos(
                "github_pat_11ABCDEF",
                &OperationId::from("test"),
                &NoOpProgressReporter,
            )
            .await
            .unwrap();

        assert_eq!(repos.len(), 101);
        assert_eq!(repos[100].full_name, "org/repo100");
    }

    #[tokio::test]
    async fn fine_grained_pat_handles_empty_installation() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/installation/repositories")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"total_count": 0, "repositories": []}"#)
            .create_async()
            .await;

        let repos = client_for(&server)
            .list_user_repos(
                "github_pat_11ABCDEF",
                &OperationId::from("test"),
                &NoOpProgressReporter,
            )
            .await
            .unwrap();

        assert!(repos.is_empty());
    }

    #[tokio::test]
    async fn fine_grained_pat_never_calls_user_repos() {
        // Guard against regressions: installing a `/user/repos` mock that
        // would error on any call proves the fine-grained path never falls
        // back to the wrong endpoint.
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/user/repos")
            .match_query(mockito::Matcher::Any)
            .with_status(404)
            .with_body(r#"{"message": "should not be called"}"#)
            .create_async()
            .await;
        server
            .mock("GET", "/installation/repositories")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{"total_count": 1, "repositories": [{}]}}"#,
                repo_json(7, "org/repo")
            ))
            .create_async()
            .await;

        let repos = client_for(&server)
            .list_user_repos(
                "github_pat_11ABCDEF",
                &OperationId::from("test"),
                &NoOpProgressReporter,
            )
            .await
            .unwrap();
        assert_eq!(repos.len(), 1);
    }
}
