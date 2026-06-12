use crate::utils::{sanitize_log, truncate_for_log};
use crate::Result;
use reqwest::{Client, Method, Response};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const DEFAULT_API_BASE_URL: &str = "https://api.github.com";
const USER_AGENT: &str = "DeployKeys-Desktop/0.1.0";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const ERROR_BODY_MAX_LEN: usize = 300;

/// The authenticated GitHub user, as returned by `GET /user`.
#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub login: String,
    pub id: i64,
    pub avatar_url: Option<String>,
}

/// GitHub REST API client with timeouts and log sanitization.
pub struct GitHubClient {
    client: Client,
    base_url: String,
}

impl GitHubClient {
    /// Create a client for the public GitHub API (`https://api.github.com`).
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()?;

        Ok(Self {
            client,
            base_url: DEFAULT_API_BASE_URL.to_string(),
        })
    }

    /// Override the API base URL (tests, GitHub Enterprise).
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    fn request_builder(&self, token: &str, method: Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);

        tracing::debug!("GitHub API request: {} {}", method, sanitize_log(&url));

        self.client
            .request(method, url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
    }

    async fn check_status(response: Response) -> Result<Response> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        let body = response.text().await.unwrap_or_default();
        let sanitized = truncate_for_log(&sanitize_log(&body), ERROR_BODY_MAX_LEN);
        tracing::error!("GitHub API error: {} - {}", status, sanitized);
        Err(crate::Error::GitHub(format!(
            "GitHub API error: {} - {}",
            status, sanitized
        )))
    }

    /// Send a request without a body and deserialize the JSON response.
    pub async fn request<T: DeserializeOwned>(
        &self,
        token: &str,
        method: Method,
        path: &str,
    ) -> Result<T> {
        let response = self.request_builder(token, method, path).send().await?;
        let response = Self::check_status(response).await?;
        Ok(response.json().await?)
    }

    /// Send a request with a JSON body and deserialize the JSON response.
    pub async fn request_with_body<T: DeserializeOwned, B: Serialize>(
        &self,
        token: &str,
        method: Method,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let response = self
            .request_builder(token, method, path)
            .json(body)
            .send()
            .await?;
        let response = Self::check_status(response).await?;
        Ok(response.json().await?)
    }

    /// Send a request whose success response has no body (e.g. 204 No Content).
    pub async fn request_expect_empty(
        &self,
        token: &str,
        method: Method,
        path: &str,
    ) -> Result<()> {
        let response = self.request_builder(token, method, path).send().await?;
        Self::check_status(response).await?;
        Ok(())
    }

    /// Fetch the user the token belongs to.
    pub async fn get_authenticated_user(&self, token: &str) -> Result<User> {
        self.request(token, Method::GET, "/user").await
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

    #[tokio::test]
    async fn get_authenticated_user_parses_response() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/user")
            .match_header("authorization", "Bearer test_token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"login": "octocat", "id": 583231, "avatar_url": "https://example.com/a.png"}"#,
            )
            .create_async()
            .await;

        let user = client_for(&server)
            .get_authenticated_user("test_token")
            .await
            .unwrap();

        assert_eq!(user.login, "octocat");
        assert_eq!(user.id, 583231);
        assert_eq!(
            user.avatar_url.as_deref(),
            Some("https://example.com/a.png")
        );
    }

    #[tokio::test]
    async fn non_success_status_becomes_github_error() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/user")
            .with_status(401)
            .with_header("content-type", "application/json")
            .with_body(r#"{"message": "Bad credentials"}"#)
            .create_async()
            .await;

        let error = client_for(&server)
            .get_authenticated_user("bad_token")
            .await
            .unwrap_err();

        assert!(matches!(error, crate::Error::GitHub(_)));
        assert!(error.to_string().contains("401"));
    }

    #[tokio::test]
    async fn request_expect_empty_accepts_204() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("DELETE", "/anything")
            .with_status(204)
            .create_async()
            .await;

        client_for(&server)
            .request_expect_empty("token", Method::DELETE, "/anything")
            .await
            .unwrap();
    }
}
