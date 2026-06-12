use crate::utils::{sanitize_log, truncate_for_log};
use crate::{Error, Result};
use serde::Deserialize;
use std::time::Duration;

// Device flow endpoints live on github.com, NOT api.github.com.
// https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps#device-flow
const DEFAULT_OAUTH_BASE_URL: &str = "https://github.com";
const DEVICE_CODE_PATH: &str = "/login/device/code";
const ACCESS_TOKEN_PATH: &str = "/login/oauth/access_token";
const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Response from the device code endpoint (RFC 8628 §3.2).
#[derive(Debug, Deserialize, Clone)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    /// Seconds until `device_code` expires.
    pub expires_in: u64,
    /// Minimum seconds to wait between token polls.
    pub interval: u64,
}

/// Tokens returned by a successful device flow exchange.
#[derive(Debug, Clone)]
pub struct TokenSet {
    pub access_token: String,
    /// Present for GitHub Apps with expiring user tokens.
    pub refresh_token: Option<String>,
    /// Seconds until `access_token` expires, if the token expires.
    pub expires_in: Option<u64>,
}

/// Outcome of a single token poll (RFC 8628 §3.5).
///
/// Terminal failures (expired code, user denial) are returned as `Err`.
#[derive(Debug, Clone)]
pub enum PollResult {
    /// The user approved the request.
    Authorized(TokenSet),
    /// The user has not approved yet; poll again after the current interval.
    Pending,
    /// The server requested a slower cadence; add 5 seconds to the interval.
    SlowDown,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    error: Option<String>,
    error_description: Option<String>,
}

/// Client for the GitHub OAuth Device Flow.
pub struct DeviceFlowClient {
    client: reqwest::Client,
    base_url: String,
    client_id: String,
}

impl DeviceFlowClient {
    pub fn new(client_id: String) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent("DeployKeys-Desktop/0.1.0")
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()?;

        Ok(Self {
            client,
            base_url: DEFAULT_OAUTH_BASE_URL.to_string(),
            client_id,
        })
    }

    /// Override the OAuth base URL (tests, GitHub Enterprise).
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    /// Request a device code and user code to start the flow.
    pub async fn request_device_code(&self) -> Result<DeviceCodeResponse> {
        let url = format!("{}{}", self.base_url, DEVICE_CODE_PATH);

        let response = self
            .client
            .post(&url)
            // Without this header GitHub answers with form-encoded text.
            .header("Accept", "application/json")
            .form(&[("client_id", self.client_id.as_str())])
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let body = truncate_for_log(&sanitize_log(&body), 300);
            return Err(Error::Auth(format!(
                "Device code request failed: {} - {}",
                status, body
            )));
        }

        Ok(response.json().await?)
    }

    /// Poll the token endpoint once.
    ///
    /// The caller is responsible for pacing: wait `interval` seconds between
    /// calls, add 5 seconds after [`PollResult::SlowDown`], and stop once
    /// `expires_in` has elapsed.
    pub async fn poll_for_token(&self, device_code: &str) -> Result<PollResult> {
        let url = format!("{}{}", self.base_url, ACCESS_TOKEN_PATH);

        let response = self
            .client
            .post(&url)
            .header("Accept", "application/json")
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("device_code", device_code),
                ("grant_type", DEVICE_GRANT_TYPE),
            ])
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let body = truncate_for_log(&sanitize_log(&body), 300);
            return Err(Error::Auth(format!(
                "Token request failed: {} - {}",
                status, body
            )));
        }

        let body: TokenResponse = response.json().await?;

        if let Some(access_token) = body.access_token {
            return Ok(PollResult::Authorized(TokenSet {
                access_token,
                refresh_token: body.refresh_token,
                expires_in: body.expires_in,
            }));
        }

        match body.error.as_deref() {
            Some("authorization_pending") => Ok(PollResult::Pending),
            Some("slow_down") => Ok(PollResult::SlowDown),
            Some("expired_token") => Err(Error::Auth(
                "Device code expired; restart the sign-in flow".to_string(),
            )),
            Some("access_denied") => Err(Error::Auth("Access denied by user".to_string())),
            Some(other) => {
                let description = body.error_description.unwrap_or_default();
                Err(Error::Auth(format!(
                    "OAuth error: {} {}",
                    other, description
                )))
            }
            None => Err(Error::Auth(
                "OAuth response contained neither a token nor an error".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client_for(server: &mockito::Server) -> DeviceFlowClient {
        DeviceFlowClient::new("test_client_id".to_string())
            .expect("client builds")
            .with_base_url(server.url())
    }

    #[tokio::test]
    async fn request_device_code_parses_response() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/login/device/code")
            .match_header("accept", "application/json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "device_code": "dc_123",
                    "user_code": "ABCD-1234",
                    "verification_uri": "https://github.com/login/device",
                    "expires_in": 900,
                    "interval": 5
                }"#,
            )
            .create_async()
            .await;

        let response = client_for(&server).request_device_code().await.unwrap();

        assert_eq!(response.device_code, "dc_123");
        assert_eq!(response.user_code, "ABCD-1234");
        assert_eq!(response.interval, 5);
        assert_eq!(response.expires_in, 900);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn poll_reports_pending() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/login/oauth/access_token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"error": "authorization_pending"}"#)
            .create_async()
            .await;

        let result = client_for(&server).poll_for_token("dc").await.unwrap();
        assert!(matches!(result, PollResult::Pending));
    }

    #[tokio::test]
    async fn poll_reports_slow_down() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/login/oauth/access_token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"error": "slow_down", "interval": 10}"#)
            .create_async()
            .await;

        let result = client_for(&server).poll_for_token("dc").await.unwrap();
        assert!(matches!(result, PollResult::SlowDown));
    }

    #[tokio::test]
    async fn poll_returns_token_set_with_refresh_token() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/login/oauth/access_token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "access_token": "ghu_access",
                    "refresh_token": "ghr_refresh",
                    "expires_in": 28800,
                    "token_type": "bearer",
                    "scope": ""
                }"#,
            )
            .create_async()
            .await;

        let result = client_for(&server).poll_for_token("dc").await.unwrap();
        match result {
            PollResult::Authorized(tokens) => {
                assert_eq!(tokens.access_token, "ghu_access");
                assert_eq!(tokens.refresh_token.as_deref(), Some("ghr_refresh"));
                assert_eq!(tokens.expires_in, Some(28800));
            }
            other => panic!("expected Authorized, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn poll_maps_expired_token_to_error() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/login/oauth/access_token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"error": "expired_token"}"#)
            .create_async()
            .await;

        let error = client_for(&server).poll_for_token("dc").await.unwrap_err();
        assert!(matches!(error, Error::Auth(_)));
        assert!(error.to_string().contains("expired"));
    }

    #[tokio::test]
    async fn poll_maps_access_denied_to_error() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/login/oauth/access_token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"error": "access_denied"}"#)
            .create_async()
            .await;

        let error = client_for(&server).poll_for_token("dc").await.unwrap_err();
        assert!(matches!(error, Error::Auth(_)));
    }
}
