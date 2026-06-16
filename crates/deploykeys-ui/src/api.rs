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

/// Returns the persisted session account, if the user is already signed in.
pub async fn get_session() -> Result<Option<Account>, String> {
    invoke_no_args("get_session").await
}

pub async fn get_language() -> Result<Option<String>, String> {
    invoke_no_args("get_language").await
}

pub async fn set_language(code: &str) -> Result<(), String> {
    #[derive(Serialize)]
    struct Args<'a> {
        code: &'a str,
    }
    invoke("set_language", &Args { code }).await
}

pub async fn get_page_size() -> Result<Option<usize>, String> {
    invoke_no_args("get_page_size").await
}

pub async fn set_page_size(size: usize) -> Result<(), String> {
    #[derive(Serialize)]
    struct Args {
        size: usize,
    }
    invoke("set_page_size", &Args { size }).await
}

/// Sign in with a Personal Access Token; returns the account on success.
pub async fn sign_in_with_token(token: &str) -> Result<Account, String> {
    #[derive(Serialize)]
    struct Args<'a> {
        token: &'a str,
    }
    invoke("sign_in_with_token", &Args { token }).await
}

pub async fn open_url(url: &str) -> Result<(), String> {
    #[derive(Serialize)]
    struct Args<'a> {
        url: &'a str,
    }
    invoke("open_url", &Args { url }).await
}

/// Clear the persisted session on the backend (account row + keyring token).
pub async fn sign_out() -> Result<(), String> {
    invoke_no_args("sign_out").await
}

/// A repository as shown in the Repos list. Mirrors the backend `RepoDto`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Repo {
    pub full_name: String,
    pub owner: String,
    pub name: String,
    pub private: bool,
    pub archived: bool,
    pub language: Option<String>,
    pub default_branch: Option<String>,
    pub html_url: String,
    pub ssh_url: String,
}

/// Count returned by `sync_repositories`. Mirrors the backend `SyncSummaryDto`.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct RepoSyncResult {
    #[allow(dead_code)]
    pub repositories: usize,
}

/// Read the locally-synced repositories (no network).
pub async fn list_repositories() -> Result<Vec<Repo>, String> {
    invoke_no_args("list_repositories").await
}

/// Refresh repositories from GitHub for the active account, then persist them.
pub async fn sync_repositories() -> Result<RepoSyncResult, String> {
    invoke_no_args("sync_repositories").await
}

/// An SSH key as shown in the Keys list.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SshKey {
    pub id: i64,
    pub directory: String,
    pub algorithm: String,
    pub comment: String,
    pub remark: String,
    pub created_at: String,
}

/// List all SSH keys.
pub async fn list_ssh_keys() -> Result<Vec<SshKey>, String> {
    invoke_no_args("list_ssh_keys").await
}

/// Create a new SSH key pair.
pub async fn create_ssh_key(
    directory: String,
    algorithm: String,
    comment: String,
    remark: String,
) -> Result<SshKey, String> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        directory: String,
        algorithm: String,
        comment: String,
        remark: String,
    }
    invoke(
        "create_ssh_key",
        &Args {
            directory,
            algorithm,
            comment,
            remark,
        },
    )
    .await
}

/// Delete an SSH key and its files.
pub async fn delete_ssh_key(id: i64) -> Result<(), String> {
    #[derive(Serialize)]
    struct Args {
        id: i64,
    }
    invoke("delete_ssh_key", &Args { id }).await
}

/// Edit an SSH key's directory and remark.
pub async fn update_ssh_key(id: i64, directory: String, remark: String) -> Result<SshKey, String> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        id: i64,
        directory: String,
        remark: String,
    }
    invoke(
        "update_ssh_key",
        &Args {
            id,
            directory,
            remark,
        },
    )
    .await
}

/// Get the public key file content for copying.
pub async fn get_public_key_content(id: i64) -> Result<String, String> {
    #[derive(Serialize)]
    struct Args {
        id: i64,
    }
    invoke("get_public_key_content", &Args { id }).await
}

/// Check whether the key directory and expected key files still exist.
pub async fn ssh_key_files_exist(id: i64) -> Result<bool, String> {
    #[derive(Serialize)]
    struct Args {
        id: i64,
    }
    invoke("ssh_key_files_exist", &Args { id }).await
}
