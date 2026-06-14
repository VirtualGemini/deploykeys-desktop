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

/// A repository as returned by `GET /user/repos`.
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

impl GitHubClient {
    /// List every repository the token can access (owner, collaborator, and
    /// organization member), across all pages. Progress is reported as pages
    /// are fetched and, when available, as local persistence proceeds.
    pub async fn list_user_repos<P: ProgressReporter>(
        &self,
        token: &str,
        operation: &OperationId,
        progress: &P,
    ) -> Result<Vec<GitHubRepository>> {
        let mut all = Vec::new();
        progress.report(operation.clone(), 5);
        for page in 1..=MAX_PAGES {
            let path = format!("/user/repos?per_page={}&page={}", PER_PAGE, page);
            // `/user/repos` returns a bare array (no wrapper), so stop once a
            // page comes back shorter than the page size.
            let repos: Vec<GitHubRepository> = self.request(token, Method::GET, &path).await?;

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
}
