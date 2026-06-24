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

/// GitHub App installation tokens (`ghs_...`) are the *only* tokens served by
/// `GET /installation/repositories`. That endpoint requires an installation
/// access token and rejects everything else with 403.
///
/// Fine-grained PATs (`github_pat_...`) are **personal** tokens: despite the
/// name, they are NOT installation tokens and must use `GET /user/repos` like
/// classic PATs (`ghp_...`) and OAuth tokens (`gho_...`). Routing them to the
/// installation endpoint breaks the personal-PAT sign-in flow.
fn is_installation_token(token: &str) -> bool {
    token.starts_with("ghs_")
}

impl GitHubClient {
    /// List every repository the token can access, across all pages. Progress
    /// is reported as pages are fetched and, when available, as local
    /// persistence proceeds.
    ///
    /// GitHub App installation tokens (`ghs_...`) are routed to
    /// `GET /installation/repositories`; every other token type — classic PATs,
    /// fine-grained PATs, and OAuth tokens — keeps using `GET /user/repos`.
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
            let repos = if is_installation_token(token) {
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

/// Pick the listing endpoint based on the token type. Only GitHub App
/// installation tokens (`ghs_...`) use the installation endpoint; every other
/// token — including fine-grained PATs — uses `/user/repos`.
fn endpoint_path(token: &str) -> &'static str {
    if is_installation_token(token) {
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

    // ---- installation token routing ----------------------------------------

    #[test]
    fn installation_token_detected_by_prefix() {
        // Only `ghs_` tokens are GitHub App installation tokens.
        assert!(is_installation_token("ghs_abcdefghijklmnopqrstuvwxyz"));
        // Fine-grained PATs are personal tokens, NOT installation tokens.
        assert!(!is_installation_token("github_pat_11ABCDE0123456789"));
        assert!(!is_installation_token("ghp_abcdefghijklmnopqrstuvwxyz"));
        assert!(!is_installation_token("gho_abcdefghijklmnopqrstuvwxyz"));
        assert!(!is_installation_token("token"));
    }

    #[test]
    fn endpoint_path_routes_by_token_type() {
        // Only installation tokens reach the installation endpoint.
        assert_eq!(
            endpoint_path("ghs_abcdefghijklmnopqrstuvwxyz"),
            "/installation/repositories"
        );
        // Every other token type — including fine-grained PATs — uses
        // /user/repos. This is the regression guard: routing fine-grained
        // PATs to /installation/repositories 403s on GitHub.
        assert_eq!(endpoint_path("github_pat_11ABCDE0123456789"), "/user/repos");
        assert_eq!(endpoint_path("ghp_token"), "/user/repos");
        assert_eq!(endpoint_path("gho_token"), "/user/repos");
        assert_eq!(endpoint_path("anything_else"), "/user/repos");
    }

    #[tokio::test]
    async fn installation_token_uses_installation_endpoint() {
        let mut server = mockito::Server::new_async().await;
        // An installation token must hit `/installation/repositories`, NOT
        // `/user/repos`. Mock only the installation path so a stray
        // `/user/repos` call would fail with a 404 -> error.
        server
            .mock("GET", "/installation/repositories")
            .match_query(mockito::Matcher::Any)
            .match_header("authorization", "Bearer ghs_installtoken")
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
                "ghs_installtoken",
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
    async fn installation_token_paginates_installation_endpoint() {
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
                "ghs_installtoken",
                &OperationId::from("test"),
                &NoOpProgressReporter,
            )
            .await
            .unwrap();

        assert_eq!(repos.len(), 101);
        assert_eq!(repos[100].full_name, "org/repo100");
    }

    #[tokio::test]
    async fn installation_token_handles_empty_installation() {
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
                "ghs_installtoken",
                &OperationId::from("test"),
                &NoOpProgressReporter,
            )
            .await
            .unwrap();

        assert!(repos.is_empty());
    }

    #[tokio::test]
    async fn installation_token_never_calls_user_repos() {
        // Guard against regressions: installing a `/user/repos` mock that
        // would error on any call proves the installation path never falls
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
                "ghs_installtoken",
                &OperationId::from("test"),
                &NoOpProgressReporter,
            )
            .await
            .unwrap();
        assert_eq!(repos.len(), 1);
    }

    #[tokio::test]
    async fn fine_grained_pat_uses_user_repos_not_installation() {
        // Regression guard: fine-grained PATs are personal tokens and must go
        // through `/user/repos`. Routing them to `/installation/repositories`
        // gets a 403 from GitHub ("must authenticate with an installation
        // access token"). Mock only `/user/repos`; a stray call to the
        // installation endpoint would hit no mock and error out.
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/user/repos")
            .match_query(mockito::Matcher::Any)
            .match_header("authorization", "Bearer github_pat_11ABCDEF")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!("[{}]", repo_json(9, "me/my-repo")))
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
        assert_eq!(repos[0].full_name, "me/my-repo");
        assert_eq!(repos[0].owner.login, "me");
    }
}
