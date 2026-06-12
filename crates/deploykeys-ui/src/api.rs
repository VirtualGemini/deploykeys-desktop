//! UI-side view of the Tauri command surface.
//!
//! The UI is plain CSR wasm and cannot depend on `deploykeys-core` (it pulls in
//! tokio/sqlx/keyring, all native-only). So we mirror just the fields the UI
//! needs as local DTOs and deserialize the command results into them.

use crate::tauri::{invoke, invoke_no_args};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Account {
    pub login: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    /// Minimum seconds between polls.
    pub interval: u64,
    /// Seconds until the device code expires.
    #[allow(dead_code)]
    pub expires_in: u64,
}

/// Outcome of a single token poll. Mirrors the backend `PollDto` tagged enum.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Poll {
    Pending,
    SlowDown,
    Authorized { account: Account },
}

/// Returns the persisted session account, if the user is already signed in.
pub async fn get_session() -> Result<Option<Account>, String> {
    invoke_no_args("get_session").await
}

pub async fn get_language() -> Result<Option<String>, String> {
    invoke_no_args("get_language").await
}

/// Reserved for a future settings screen; the language currently follows the
/// persisted preference with no in-app picker yet.
#[allow(dead_code)]
pub async fn set_language(code: &str) -> Result<(), String> {
    #[derive(Serialize)]
    struct Args<'a> {
        code: &'a str,
    }
    invoke("set_language", &Args { code }).await
}

pub async fn start_github_auth() -> Result<DeviceCode, String> {
    invoke_no_args("start_github_auth").await
}

pub async fn poll_github_auth(device_code: &str) -> Result<Poll, String> {
    #[derive(Serialize)]
    struct Args<'a> {
        #[serde(rename = "deviceCode")]
        device_code: &'a str,
    }
    invoke("poll_github_auth", &Args { device_code }).await
}

pub async fn open_url(url: &str) -> Result<(), String> {
    #[derive(Serialize)]
    struct Args<'a> {
        url: &'a str,
    }
    invoke("open_url", &Args { url }).await
}
