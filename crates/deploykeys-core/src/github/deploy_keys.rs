use crate::{Error, Result};
use reqwest::Method;
use serde::{Deserialize, Serialize};

use super::GitHubClient;

#[derive(Debug, Serialize)]
pub struct CreateDeployKeyRequest {
    pub title: String,
    pub key: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeployKey {
    pub id: i64,
    pub key: String,
    pub url: String,
    pub title: String,
    pub verified: bool,
    pub created_at: String,
    pub read_only: bool,
}

/// Reject values that cannot be a GitHub owner or repository name.
///
/// Defense in depth: callers normally pass values that came from the GitHub
/// API, but this prevents path manipulation if they ever do not.
fn validate_path_segment(value: &str, field: &str) -> Result<()> {
    let chars_ok = value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));

    if value.is_empty() || value.len() > 100 || !chars_ok || value == "." || value == ".." {
        return Err(Error::Validation(format!(
            "Invalid GitHub {}: {:?}",
            field, value
        )));
    }

    Ok(())
}

fn keys_path(owner: &str, repo: &str) -> Result<String> {
    validate_path_segment(owner, "owner")?;
    validate_path_segment(repo, "repository name")?;
    Ok(format!("/repos/{}/{}/keys", owner, repo))
}

impl GitHubClient {
    pub async fn create_deploy_key(
        &self,
        token: &str,
        owner: &str,
        repo: &str,
        request: &CreateDeployKeyRequest,
    ) -> Result<DeployKey> {
        let path = keys_path(owner, repo)?;
        self.request_with_body(token, Method::POST, &path, request)
            .await
    }

    pub async fn list_deploy_keys(
        &self,
        token: &str,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<DeployKey>> {
        let path = keys_path(owner, repo)?;
        self.request(token, Method::GET, &path).await
    }

    /// Delete a deploy key. GitHub answers 204 No Content on success.
    pub async fn delete_deploy_key(
        &self,
        token: &str,
        owner: &str,
        repo: &str,
        key_id: i64,
    ) -> Result<()> {
        let path = format!("{}/{}", keys_path(owner, repo)?, key_id);
        self.request_expect_empty(token, Method::DELETE, &path)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client_for(server: &mockito::Server) -> GitHubClient {
        GitHubClient::new()
            .expect("client builds")
            .with_base_url(server.url())
    }

    const DEPLOY_KEY_JSON: &str = r#"{
        "id": 1234,
        "key": "ssh-ed25519 AAAA test",
        "url": "https://api.github.com/repos/owner/repo/keys/1234",
        "title": "deploykeys",
        "verified": true,
        "created_at": "2026-06-11T00:00:00Z",
        "read_only": true
    }"#;

    #[tokio::test]
    async fn create_deploy_key_posts_and_parses() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/repos/owner/repo/keys")
            .match_header("authorization", "Bearer token")
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(DEPLOY_KEY_JSON)
            .create_async()
            .await;

        let request = CreateDeployKeyRequest {
            title: "deploykeys".to_string(),
            key: "ssh-ed25519 AAAA test".to_string(),
            read_only: true,
        };

        let key = client_for(&server)
            .create_deploy_key("token", "owner", "repo", &request)
            .await
            .unwrap();

        assert_eq!(key.id, 1234);
        assert!(key.read_only);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn list_deploy_keys_parses_array() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/repos/owner/repo/keys")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!("[{}]", DEPLOY_KEY_JSON))
            .create_async()
            .await;

        let keys = client_for(&server)
            .list_deploy_keys("token", "owner", "repo")
            .await
            .unwrap();

        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].id, 1234);
    }

    #[tokio::test]
    async fn delete_deploy_key_accepts_204_empty_body() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("DELETE", "/repos/owner/repo/keys/1234")
            .with_status(204)
            .create_async()
            .await;

        client_for(&server)
            .delete_deploy_key("token", "owner", "repo", 1234)
            .await
            .unwrap();

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn path_traversal_in_owner_is_rejected_without_http_call() {
        let server = mockito::Server::new_async().await;

        let error = client_for(&server)
            .delete_deploy_key("token", "../../user", "repo", 1)
            .await
            .unwrap_err();

        assert!(matches!(error, Error::Validation(_)));
    }

    #[tokio::test]
    async fn empty_repo_name_is_rejected() {
        let server = mockito::Server::new_async().await;

        let error = client_for(&server)
            .list_deploy_keys("token", "owner", "")
            .await
            .unwrap_err();

        assert!(matches!(error, Error::Validation(_)));
    }
}
