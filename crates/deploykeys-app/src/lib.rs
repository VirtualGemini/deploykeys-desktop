//! Tauri backend for DeployKeys Desktop.
//!
//! Hosts the native side of the app: opens the database, exposes the IPC
//! command surface the Leptos webview calls, and bridges to `deploykeys-core`
//! (PAT sign-in, repo sync, account persistence, settings). The webview itself
//! is the separate `deploykeys-ui` crate, built to wasm by Trunk.

use deploykeys_core::credentials::CredentialStore;
use deploykeys_core::db::Database;
use deploykeys_core::models::{
    Account, DeployKeyPermission, KeyAlgorithm, KeyBinding, KeyBindingStatus, Repository,
};
use deploykeys_core::progress::{OperationId, ProgressReporter};
use deploykeys_core::services::{AuthService, KeyBindingService, RepoSyncService, SshKeyService};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tokio::io::{AsyncRead, AsyncReadExt};

const PROGRESS_EVENT: &str = "progress";

/// `app_settings` key under which the chosen UI language is stored.
const LANGUAGE_SETTING_KEY: &str = "language";
/// `app_settings` key under which the repository-list page size is stored.
const PAGE_SIZE_SETTING_KEY: &str = "repos_page_size";
/// `app_settings` key under which clone task history is stored as JSON.
const CLONE_TASKS_SETTING_KEY: &str = "clone_tasks";
/// `app_settings` key under which each repository's latest successful clone
/// path is stored as JSON.
const REPO_CLONE_PATHS_SETTING_KEY: &str = "repo_clone_paths";
/// `app_settings` key under which the currently connected connection id is
/// stored (empty string = all connections offline).
const ACTIVE_CONNECTION_SETTING_KEY: &str = "active_connection";

/// Shared native state, managed by Tauri and injected into every command.
struct AppState {
    db: Database,
    /// The active session's GitHub access token, cached in memory so repeated
    /// operations (e.g. repo sync) don't re-read the OS keyring — each keyring
    /// read can trigger a macOS keychain trust prompt. Populated on sign-in and
    /// lazily on first read; cleared on sign-out.
    token: Mutex<Option<String>>,
    clone_tasks: Arc<Mutex<Vec<CloneTaskDto>>>,
    next_clone_task_id: AtomicU64,
}

impl AppState {
    fn new(db: Database, clone_tasks: Vec<CloneTaskDto>) -> Self {
        let next_clone_task_id = clone_tasks.iter().map(|task| task.id).max().unwrap_or(0) + 1;
        Self {
            db,
            token: Mutex::new(None),
            clone_tasks: Arc::new(Mutex::new(clone_tasks)),
            next_clone_task_id: AtomicU64::new(next_clone_task_id),
        }
    }

    fn cache_token(&self, token: String) {
        *self.token.lock().expect("token cache lock") = Some(token);
    }

    fn clear_token(&self) {
        *self.token.lock().expect("token cache lock") = None;
    }

    fn cached_token(&self) -> Option<String> {
        self.token.lock().expect("token cache lock").clone()
    }
}

// ---- DTOs crossing the IPC boundary --------------------------------------
//
// We send purpose-built DTOs rather than core models so secrets (keyring
// references, tokens) never reach the webview, and the wire shape is decoupled
// from the database schema.

#[derive(Serialize)]
struct AccountDto {
    login: String,
    avatar_url: Option<String>,
}

/// A repository row sent to the webview for the Repos list. Excludes internal
/// ids and keyring/permission snapshots the UI has no use for.
#[derive(Serialize)]
struct RepoDto {
    id: i64,
    full_name: String,
    owner: String,
    name: String,
    private: bool,
    archived: bool,
    language: Option<String>,
    default_branch: Option<String>,
    html_url: String,
    ssh_url: String,
}

/// Count returned after a repository sync.
#[derive(Serialize)]
struct SyncSummaryDto {
    repositories: usize,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CloneTaskStatus {
    Running,
    Succeeded,
    Failed,
}

/// A persisted clone task, including its terminal-style output.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloneTaskDto {
    id: u64,
    repo_id: i64,
    repo_full_name: String,
    repo_name: String,
    local_path: String,
    command: String,
    status: CloneTaskStatus,
    log: String,
    started_at: i64,
    finished_at: Option<i64>,
    exit_code: Option<i32>,
    error: Option<String>,
}

impl CloneTaskDto {
    fn is_running(&self) -> bool {
        self.status == CloneTaskStatus::Running
    }
}

#[derive(Clone)]
struct PreparedClone {
    task_id: u64,
    clone_url: String,
    parent_path: PathBuf,
    local_path: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RepoRemoteResultDto {
    repo_id: i64,
    repo_full_name: String,
    local_path: String,
    remote_url: String,
    output: String,
}

/// An SSH key sent to the webview for the Keys list.
#[derive(Serialize)]
struct SshKeyDto {
    id: i64,
    directory: String,
    algorithm: String,
    comment: String,
    remark: String,
    created_at: String,
}

/// Payload sent to the webview for each progress checkpoint.
#[derive(Clone, Serialize)]
struct ProgressEvent {
    operation: String,
    percent: u8,
}

/// A reporter that emits progress events to the webview through Tauri's event
/// bus. Lightweight: each checkpoint serializes the operation id + percent.
struct TauriProgressReporter {
    app: AppHandle,
}

impl TauriProgressReporter {
    fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl ProgressReporter for TauriProgressReporter {
    fn report(&self, operation: OperationId, percent: u8) {
        let _ = self.app.emit(
            PROGRESS_EVENT,
            ProgressEvent {
                operation: operation.to_string(),
                percent,
            },
        );
    }
}

// ---- Commands -------------------------------------------------------------

/// Return the persisted session account, if any. The first account row is the
/// active session (the app is single-account today).
#[tauri::command]
async fn get_session(state: State<'_, AppState>) -> Result<Option<AccountDto>, String> {
    let accounts = state
        .db
        .accounts()
        .list_all()
        .await
        .map_err(|e| e.to_string())?;
    Ok(accounts.into_iter().next().map(|a| AccountDto {
        login: a.login,
        avatar_url: a.avatar_url,
    }))
}

#[tauri::command]
async fn get_page_size(state: State<'_, AppState>) -> Result<Option<usize>, String> {
    match state.db.get_setting(PAGE_SIZE_SETTING_KEY).await {
        Ok(Some(value)) => match value.parse::<usize>() {
            Ok(size) => Ok(Some(size)),
            Err(_) => Ok(None),
        },
        Ok(None) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
async fn set_page_size(state: State<'_, AppState>, size: usize) -> Result<(), String> {
    state
        .db
        .set_setting(PAGE_SIZE_SETTING_KEY, &size.to_string())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_language(state: State<'_, AppState>) -> Result<Option<String>, String> {
    state
        .db
        .get_setting(LANGUAGE_SETTING_KEY)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_language(state: State<'_, AppState>, code: String) -> Result<(), String> {
    state
        .db
        .set_setting(LANGUAGE_SETTING_KEY, &code)
        .await
        .map_err(|e| e.to_string())
}

/// Read the persisted active connection id. `None` means it was never set (the
/// UI then falls back to its default); an empty string means all offline.
#[tauri::command]
async fn get_active_connection(state: State<'_, AppState>) -> Result<Option<String>, String> {
    state
        .db
        .get_setting(ACTIVE_CONNECTION_SETTING_KEY)
        .await
        .map_err(|e| e.to_string())
}

/// Persist the active connection id. Pass an empty string to record that all
/// connections are offline.
#[tauri::command]
async fn set_active_connection(state: State<'_, AppState>, value: String) -> Result<(), String> {
    state
        .db
        .set_setting(ACTIVE_CONNECTION_SETTING_KEY, &value)
        .await
        .map_err(|e| e.to_string())
}

/// Sign in with a Personal Access Token: validate it, persist the account, and
/// cache the token in memory for this session. Returns the account.
#[tauri::command]
async fn sign_in_with_token(
    app: AppHandle,
    state: State<'_, AppState>,
    token: String,
) -> Result<AccountDto, String> {
    let service = AuthService::new(state.db.clone()).map_err(|e| e.to_string())?;
    let reporter = TauriProgressReporter::new(app);
    let account = match service.sign_in_with_token(token.clone(), &reporter).await {
        Ok(account) => account,
        Err(e) => {
            tracing::error!("PAT sign-in failed: {}", e);
            return Err(e.to_string());
        }
    };
    state.cache_token(token);
    Ok(AccountDto {
        login: account.login,
        avatar_url: account.avatar_url,
    })
}

/// Open a URL in the user's default browser. Failure is reported but rarely
/// matters — the UI still shows the code to copy manually.
#[tauri::command]
async fn open_url(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| e.to_string())
}

/// Sign out: clear the persisted session so it does not resurface on the next
/// launch. Removes the keyring token and the account row (which cascades to
/// repositories and key bindings).
#[tauri::command]
async fn sign_out(state: State<'_, AppState>) -> Result<(), String> {
    let service = AuthService::new(state.db.clone()).map_err(|e| e.to_string())?;
    service.sign_out().await.map_err(|e| e.to_string())?;
    state.clear_token();
    Ok(())
}

/// Return the locally-synced repositories, sorted by full name. Read-only:
/// call `sync_repositories` first to refresh from GitHub.
#[tauri::command]
async fn list_repositories(state: State<'_, AppState>) -> Result<Vec<RepoDto>, String> {
    let repos = state
        .db
        .repositories()
        .list_all()
        .await
        .map_err(|e| e.to_string())?;
    Ok(repos
        .into_iter()
        .map(|r| RepoDto {
            id: r.id,
            full_name: r.full_name,
            owner: r.owner,
            name: r.name,
            private: r.private,
            archived: r.archived,
            language: r.language,
            default_branch: r.default_branch,
            html_url: r.html_url,
            ssh_url: r.ssh_url,
        })
        .collect())
}

/// Pull the account's repositories (`GET /user/repos`) from GitHub and persist
/// them. The UI then calls `list_repositories` to render the result.
#[tauri::command]
async fn sync_repositories(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SyncSummaryDto, String> {
    let account = state
        .db
        .accounts()
        .list_all()
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .ok_or("Not signed in")?;

    let token = resolve_token(&state, &account).await?;

    let service = RepoSyncService::new(state.db.clone()).map_err(|e| e.to_string())?;
    let reporter = TauriProgressReporter::new(app);
    let repositories = match service.sync_repos(account.id, &token, &reporter).await {
        Ok(count) => count,
        Err(e) => {
            tracing::error!("Repository sync failed: {}", e);
            return Err(e.to_string());
        }
    };
    Ok(SyncSummaryDto { repositories })
}

/// List all SSH keys for the local target.
#[tauri::command]
async fn list_ssh_keys(state: State<'_, AppState>) -> Result<Vec<SshKeyDto>, String> {
    let service = SshKeyService::new(state.db.clone());
    let keys = service.list_all_keys().await.map_err(|e| e.to_string())?;

    Ok(keys.into_iter().map(ssh_key_dto).collect())
}

/// Create a new SSH key pair.
#[tauri::command]
async fn create_ssh_key(
    state: State<'_, AppState>,
    directory: String,
    algorithm: String,
    comment: String,
    remark: String,
) -> Result<SshKeyDto, String> {
    let algo: KeyAlgorithm = algorithm
        .parse()
        .map_err(|e| format!("Invalid algorithm: {}", e))?;

    let service = SshKeyService::new(state.db.clone());
    let key = service
        .create_key(directory, algo, comment, remark)
        .await
        .map_err(|e| e.to_string())?;

    Ok(ssh_key_dto(key))
}

/// Delete an SSH key and its files.
#[tauri::command]
async fn delete_ssh_key(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let service = SshKeyService::new(state.db.clone());
    service.delete_key(id).await.map_err(|e| e.to_string())
}

/// Edit an SSH key's directory and remark. The directory rename moves the
/// key's folder on disk and rewrites the stored file paths.
#[tauri::command]
async fn update_ssh_key(
    state: State<'_, AppState>,
    id: i64,
    directory: String,
    remark: String,
) -> Result<SshKeyDto, String> {
    let service = SshKeyService::new(state.db.clone());
    let key = service
        .update_key(id, &directory, &remark)
        .await
        .map_err(|e| e.to_string())?;

    Ok(ssh_key_dto(key))
}

/// Get the public key file content for copying to clipboard.
#[tauri::command]
async fn get_public_key_content(state: State<'_, AppState>, id: i64) -> Result<String, String> {
    let service = SshKeyService::new(state.db.clone());
    service.read_public_key(id).await.map_err(|e| e.to_string())
}

/// Check whether the key's directory and expected key files still exist.
#[tauri::command]
async fn ssh_key_files_exist(state: State<'_, AppState>, id: i64) -> Result<bool, String> {
    let service = SshKeyService::new(state.db.clone());
    service.key_files_exist(id).await.map_err(|e| e.to_string())
}

/// Upload an existing local SSH key's public key to GitHub as a deploy key,
/// persist the binding, and maintain the per-repository ~/.ssh/config host.
#[tauri::command]
async fn bind_deploy_key(
    state: State<'_, AppState>,
    repo_id: i64,
    ssh_key_id: i64,
    writable: bool,
) -> Result<(), String> {
    let repo = state
        .db
        .repositories()
        .find_by_id(repo_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Repository not found")?;
    let account = state
        .db
        .accounts()
        .find_by_id(repo.account_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Account not found")?;
    let token = resolve_token(&state, &account).await?;
    let permission = if writable {
        DeployKeyPermission::ReadWrite
    } else {
        DeployKeyPermission::ReadOnly
    };

    let service = KeyBindingService::new(state.db.clone()).map_err(|e| e.to_string())?;
    service
        .upload_existing_key(repo_id, ssh_key_id, &token, permission)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Let the user pick a parent directory, then clone the repository into
/// `<selected directory>/<repository name>`.
#[tauri::command]
async fn clone_repository(
    app: AppHandle,
    state: State<'_, AppState>,
    repo_id: i64,
    title: String,
) -> Result<Option<CloneTaskDto>, String> {
    let repo = state
        .db
        .repositories()
        .find_by_id(repo_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Repository not found")?;
    validate_repo_name(&repo.name)?;

    let mut dialog = app.dialog().file().set_title(title);
    if let Some(home_dir) = dirs::home_dir() {
        dialog = dialog.set_directory(home_dir);
    }

    let Some(parent) = dialog.blocking_pick_folder() else {
        return Ok(None);
    };
    let parent = parent.into_path().map_err(|e| e.to_string())?;
    if !parent.is_dir() {
        return Err(format!(
            "Selected path is not a directory: {}",
            parent.display()
        ));
    }

    let prepared = prepare_clone_task(&state, &repo, parent).await?;
    let task = new_clone_task(&repo, &prepared);
    push_clone_task(&state.clone_tasks, task.clone());
    persist_clone_tasks(&state.db, &state.clone_tasks).await?;

    let db = state.db.clone();
    let clone_tasks = state.clone_tasks.clone();
    tokio::spawn(async move {
        run_git_clone_task(db, clone_tasks, prepared).await;
    });

    Ok(Some(task))
}

/// Return persisted clone tasks, newest first.
#[tauri::command]
async fn list_clone_tasks(state: State<'_, AppState>) -> Result<Vec<CloneTaskDto>, String> {
    Ok(snapshot_clone_tasks(&state.clone_tasks))
}

/// Clear completed clone tasks while keeping any in-flight clone visible.
#[tauri::command]
async fn clear_clone_tasks(state: State<'_, AppState>) -> Result<Vec<CloneTaskDto>, String> {
    let snapshot = {
        let mut tasks = state.clone_tasks.lock().expect("clone task lock");
        tasks.retain(CloneTaskDto::is_running);
        tasks.clone()
    };
    persist_clone_task_snapshot(&state.db, &snapshot).await?;
    Ok(snapshot)
}

/// Point the latest successful local clone's `origin` at the deploy-key SSH
/// alias for this repository.
#[tauri::command]
async fn connect_repository_remote(
    state: State<'_, AppState>,
    repo_id: i64,
) -> Result<RepoRemoteResultDto, String> {
    let repo = find_repository(&state, repo_id).await?;
    active_local_binding(&state, &repo).await?;
    let local_path = latest_successful_clone_path(&state, repo_id).await?;
    let remote_url = deploy_key_remote_url(&repo);

    let args = vec![
        "-C".to_string(),
        path_arg(&local_path),
        "remote".to_string(),
        "set-url".to_string(),
        "origin".to_string(),
        remote_url.clone(),
    ];
    run_git_checked(&args, None).await?;

    Ok(remote_result(
        &repo,
        &local_path,
        &remote_url,
        String::new(),
    ))
}

/// Test that the latest successful local clone can reach `origin`.
#[tauri::command]
async fn test_repository_remote(
    state: State<'_, AppState>,
    repo_id: i64,
) -> Result<RepoRemoteResultDto, String> {
    let repo = find_repository(&state, repo_id).await?;
    let binding = active_local_binding(&state, &repo).await?;
    let local_path = latest_successful_clone_path(&state, repo_id).await?;
    let remote_url = deploy_key_remote_url(&repo);

    ensure_git_remote_matches(&local_path, &remote_url).await?;

    let ssh_command = format!(
        "ssh -i {} -o IdentitiesOnly=yes",
        shell_quote(&binding.private_key_path)
    );
    let args = vec![
        "-C".to_string(),
        path_arg(&local_path),
        "ls-remote".to_string(),
        "origin".to_string(),
    ];
    let output = run_git_checked(&args, Some(("GIT_SSH_COMMAND", ssh_command.as_str()))).await?;

    Ok(remote_result(&repo, &local_path, &remote_url, output))
}

async fn prepare_clone_task(
    state: &AppState,
    repo: &Repository,
    parent_path: PathBuf,
) -> Result<PreparedClone, String> {
    let local_path = parent_path.join(&repo.name);
    if tokio::fs::try_exists(&local_path)
        .await
        .map_err(|e| e.to_string())?
    {
        return Err(format!(
            "Target directory already exists: {}",
            local_path.display()
        ));
    }

    let clone_url = clone_url_for_repo(state, repo).await?;
    Ok(PreparedClone {
        task_id: state.next_clone_task_id.fetch_add(1, Ordering::Relaxed),
        clone_url,
        parent_path,
        local_path,
    })
}

fn ssh_key_dto(key: deploykeys_core::models::SshKey) -> SshKeyDto {
    SshKeyDto {
        id: key.id,
        directory: key.directory,
        algorithm: key.algorithm.to_string(),
        comment: key.comment,
        remark: key.remark,
        created_at: key.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
    }
}

/// Get the active session's access token: from the in-memory cache if present,
/// otherwise read it once from the keyring and cache it. A keyring read can
/// trigger a macOS keychain prompt; surfacing a clear message here is what makes
/// a denied prompt visible instead of a silent failure.
async fn resolve_token(state: &AppState, account: &Account) -> Result<String, String> {
    if let Some(token) = state.cached_token() {
        return Ok(token);
    }

    let token_ref = account.token_ref.clone();
    let token = tokio::task::spawn_blocking(move || CredentialStore::get_token(&token_ref))
        .await
        .map_err(|e| format!("Keyring task failed: {e}"))?
        .map_err(|e| {
            tracing::error!("Could not read login token from keychain: {}", e);
            format!(
                "无法读取登录凭证：{e}。如果系统弹出钥匙串访问提示，请选择「允许」或「始终允许」。"
            )
        })?;

    state.cache_token(token.clone());
    Ok(token)
}

async fn clone_url_for_repo(state: &AppState, repo: &Repository) -> Result<String, String> {
    let bindings = state
        .db
        .key_bindings()
        .list_by_repo(repo.id)
        .await
        .map_err(|e| e.to_string())?;

    for binding in bindings {
        if binding.status != KeyBindingStatus::Active {
            continue;
        }
        if tokio::fs::try_exists(&binding.private_key_path)
            .await
            .unwrap_or(false)
        {
            return Ok(format!(
                "git@{}:{}/{}.git",
                repo_ssh_host_alias(repo),
                repo.owner,
                repo.name
            ));
        }
    }

    Ok(repo.ssh_url.clone())
}

async fn find_repository(state: &AppState, repo_id: i64) -> Result<Repository, String> {
    state
        .db
        .repositories()
        .find_by_id(repo_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Repository not found".to_string())
}

async fn active_local_binding(state: &AppState, repo: &Repository) -> Result<KeyBinding, String> {
    let bindings = state
        .db
        .key_bindings()
        .list_by_repo(repo.id)
        .await
        .map_err(|e| e.to_string())?;

    for binding in bindings {
        if binding.status != KeyBindingStatus::Active {
            continue;
        }
        if tokio::fs::try_exists(&binding.private_key_path)
            .await
            .unwrap_or(false)
        {
            return Ok(binding);
        }
    }

    Err(format!(
        "No active local deploy key is available for {}.",
        repo.full_name
    ))
}

async fn latest_successful_clone_path(state: &AppState, repo_id: i64) -> Result<PathBuf, String> {
    if let Some(path) = latest_persisted_clone_path(&state.db, repo_id).await? {
        if tokio::fs::try_exists(&path).await.unwrap_or(false) {
            return Ok(path);
        }
    }

    let tasks = snapshot_clone_tasks(&state.clone_tasks);
    let mut latest_missing_path = None::<PathBuf>;

    for task in tasks
        .into_iter()
        .filter(|task| task.repo_id == repo_id && task.status == CloneTaskStatus::Succeeded)
    {
        let path = PathBuf::from(&task.local_path);
        if tokio::fs::try_exists(&path).await.unwrap_or(false) {
            return Ok(path);
        }
        latest_missing_path.get_or_insert(path);
    }

    if let Some(path) = latest_missing_path {
        return Err(format!(
            "The latest successful clone path no longer exists: {}",
            path.display()
        ));
    }

    Err("Clone this repository successfully before connecting it.".to_string())
}

async fn latest_persisted_clone_path(
    db: &Database,
    repo_id: i64,
) -> Result<Option<PathBuf>, String> {
    let paths = load_repo_clone_paths(db).await?;
    Ok(paths.get(&repo_id).map(PathBuf::from))
}

async fn persist_repo_clone_path(
    db: &Database,
    repo_id: i64,
    local_path: &str,
) -> Result<(), String> {
    let mut paths = load_repo_clone_paths(db).await?;
    paths.insert(repo_id, local_path.to_string());
    let json = serde_json::to_string(&paths).map_err(|e| e.to_string())?;
    db.set_setting(REPO_CLONE_PATHS_SETTING_KEY, &json)
        .await
        .map_err(|e| e.to_string())
}

async fn load_repo_clone_paths(db: &Database) -> Result<HashMap<i64, String>, String> {
    let Some(json) = db
        .get_setting(REPO_CLONE_PATHS_SETTING_KEY)
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(HashMap::new());
    };

    serde_json::from_str(&json).map_err(|e| e.to_string())
}

async fn ensure_git_remote_matches(local_path: &Path, expected_url: &str) -> Result<(), String> {
    let args = vec![
        "-C".to_string(),
        path_arg(local_path),
        "remote".to_string(),
        "get-url".to_string(),
        "origin".to_string(),
    ];
    let current = run_git_checked(&args, None).await?;
    let current = current.trim();
    if current == expected_url {
        return Ok(());
    }

    Err(format!(
        "Remote origin is not connected. Click Connect first. Current origin: {}",
        if current.is_empty() {
            "(empty)"
        } else {
            current
        }
    ))
}

async fn run_git_checked(args: &[String], env: Option<(&str, &str)>) -> Result<String, String> {
    let mut command = tokio::process::Command::new(git_program());
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some((key, value)) = env {
        command.env(key, value);
    }

    let output = command
        .output()
        .await
        .map_err(|e| format!("Failed to start git: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() {
        return Ok(trim_git_output(&stdout, &stderr));
    }

    Err(format_git_failure(
        args,
        output.status.code(),
        &stdout,
        &stderr,
    ))
}

fn trim_git_output(stdout: &str, stderr: &str) -> String {
    let output = if stdout.trim().is_empty() {
        stderr.trim()
    } else {
        stdout.trim()
    };
    truncate_message(output)
}

fn format_git_failure(
    args: &[String],
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> String {
    let output = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    let status = exit_code
        .map(|code| code.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let detail = if output.is_empty() {
        "No output".to_string()
    } else {
        truncate_message(output)
    };
    format!(
        "git {} failed with status {status}: {detail}",
        args.join(" ")
    )
}

fn truncate_message(message: &str) -> String {
    const MAX_LEN: usize = 4000;
    if message.len() <= MAX_LEN {
        return message.to_string();
    }
    let mut truncated = message
        .chars()
        .take(MAX_LEN.saturating_sub(16))
        .collect::<String>();
    truncated.push_str("\n...truncated");
    truncated
}

fn deploy_key_remote_url(repo: &Repository) -> String {
    format!(
        "git@{}:{}/{}.git",
        repo_ssh_host_alias(repo),
        repo.owner,
        repo.name
    )
}

fn remote_result(
    repo: &Repository,
    local_path: &Path,
    remote_url: &str,
    output: String,
) -> RepoRemoteResultDto {
    RepoRemoteResultDto {
        repo_id: repo.id,
        repo_full_name: repo.full_name.clone(),
        local_path: path_arg(local_path),
        remote_url: remote_url.to_string(),
        output,
    }
}

fn path_arg(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

async fn run_git_clone_task(
    db: Database,
    clone_tasks: Arc<Mutex<Vec<CloneTaskDto>>>,
    prepared: PreparedClone,
) {
    let mut command = tokio::process::Command::new(git_program());
    command
        .arg("clone")
        .arg("--progress")
        .arg(&prepared.clone_url)
        .current_dir(&prepared.parent_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            finish_clone_task(
                &db,
                &clone_tasks,
                prepared.task_id,
                CloneTaskStatus::Failed,
                None,
                Some(format!("Failed to start git clone: {e}")),
            )
            .await;
            return;
        }
    };

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(read_clone_stream(stdout, tx.clone()));
    }
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(read_clone_stream(stderr, tx.clone()));
    }
    drop(tx);

    let log_db = db.clone();
    let log_tasks = clone_tasks.clone();
    let log_task_id = prepared.task_id;
    let log_collector = tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            append_clone_log(&log_db, &log_tasks, log_task_id, &chunk).await;
        }
    });

    let wait_result = child.wait().await;
    let _ = log_collector.await;

    match wait_result {
        Ok(status) if status.success() => {
            finish_clone_task(
                &db,
                &clone_tasks,
                prepared.task_id,
                CloneTaskStatus::Succeeded,
                status.code(),
                None,
            )
            .await;
        }
        Ok(status) => {
            let code = status.code();
            finish_clone_task(
                &db,
                &clone_tasks,
                prepared.task_id,
                CloneTaskStatus::Failed,
                code,
                Some(format!(
                    "git clone exited with status {}",
                    code.map(|c| c.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                )),
            )
            .await;
        }
        Err(e) => {
            finish_clone_task(
                &db,
                &clone_tasks,
                prepared.task_id,
                CloneTaskStatus::Failed,
                None,
                Some(format!("Failed while waiting for git clone: {e}")),
            )
            .await;
        }
    }
}

async fn read_clone_stream<R>(mut reader: R, tx: tokio::sync::mpsc::UnboundedSender<String>)
where
    R: AsyncRead + Unpin,
{
    let mut buf = [0_u8; 4096];
    let mut pending = String::new();
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => {
                let filtered = filtered_clone_log_chunk(&pending);
                if !filtered.is_empty() {
                    let _ = tx.send(filtered);
                }
                break;
            }
            Ok(n) => {
                pending.push_str(&String::from_utf8_lossy(&buf[..n]));
                let normalized = normalize_terminal_chunk(&pending);
                let mut lines = normalized.split('\n').collect::<Vec<_>>();
                pending = lines.pop().unwrap_or_default().to_string();
                let filtered = lines
                    .into_iter()
                    .filter_map(filter_clone_log_line)
                    .map(|line| format!("{line}\n"))
                    .collect::<String>();
                if !filtered.is_empty() && tx.send(filtered).is_err() {
                    break;
                }
            }
            Err(e) => {
                let _ = tx.send(format!("\nFailed to read git output: {e}\n"));
                break;
            }
        }
    }
}

fn normalize_terminal_chunk(chunk: &str) -> String {
    chunk.replace("\r\n", "\n").replace('\r', "\n")
}

fn filtered_clone_log_chunk(chunk: &str) -> String {
    normalize_terminal_chunk(chunk)
        .lines()
        .filter_map(filter_clone_log_line)
        .map(|line| format!("{line}\n"))
        .collect()
}

fn filter_clone_log_line(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    let keep = line.starts_with("Cloning into ")
        || line.starts_with("remote: Enumerating objects:")
        || line.starts_with("remote: Total ")
        || (line.starts_with("remote: Counting objects:") && line.contains("done."))
        || (line.starts_with("remote: Compressing objects:") && line.contains("done."))
        || (line.starts_with("Receiving objects:") && line.contains("done."))
        || (line.starts_with("Resolving deltas:") && line.contains("done."))
        || line.starts_with("fatal:")
        || line.starts_with("error:")
        || line.starts_with("ssh:")
        || line.contains("Permission denied")
        || line.contains("Host key verification failed")
        || line.contains("Repository not found");

    keep.then(|| line.to_string())
}

fn new_clone_task(repo: &Repository, prepared: &PreparedClone) -> CloneTaskDto {
    let command = format!("git clone {}", prepared.clone_url);
    CloneTaskDto {
        id: prepared.task_id,
        repo_id: repo.id,
        repo_full_name: repo.full_name.clone(),
        repo_name: repo.name.clone(),
        local_path: prepared.local_path.to_string_lossy().to_string(),
        command: command.clone(),
        status: CloneTaskStatus::Running,
        log: String::new(),
        started_at: now_secs(),
        finished_at: None,
        exit_code: None,
        error: None,
    }
}

fn push_clone_task(tasks: &Arc<Mutex<Vec<CloneTaskDto>>>, task: CloneTaskDto) {
    tasks.lock().expect("clone task lock").insert(0, task);
}

fn snapshot_clone_tasks(tasks: &Arc<Mutex<Vec<CloneTaskDto>>>) -> Vec<CloneTaskDto> {
    tasks.lock().expect("clone task lock").clone()
}

async fn append_clone_log(
    db: &Database,
    tasks: &Arc<Mutex<Vec<CloneTaskDto>>>,
    task_id: u64,
    chunk: &str,
) {
    let snapshot = {
        let mut tasks = tasks.lock().expect("clone task lock");
        if let Some(task) = tasks.iter_mut().find(|task| task.id == task_id) {
            task.log.push_str(chunk);
        }
        tasks.clone()
    };
    if let Err(e) = persist_clone_task_snapshot(db, &snapshot).await {
        tracing::warn!("Could not persist clone log: {}", e);
    }
}

async fn finish_clone_task(
    db: &Database,
    tasks: &Arc<Mutex<Vec<CloneTaskDto>>>,
    task_id: u64,
    status: CloneTaskStatus,
    exit_code: Option<i32>,
    error: Option<String>,
) {
    let should_persist_clone_path = status == CloneTaskStatus::Succeeded;
    let snapshot = {
        let mut tasks = tasks.lock().expect("clone task lock");
        if let Some(task) = tasks.iter_mut().find(|task| task.id == task_id) {
            task.status = status.clone();
            task.finished_at = Some(now_secs());
            task.exit_code = exit_code;
            task.error = error.clone();
            if let Some(error) = &error {
                if !task.log.ends_with('\n') {
                    task.log.push('\n');
                }
                task.log.push_str(error);
                task.log.push('\n');
            }
        }
        tasks.clone()
    };
    if should_persist_clone_path {
        if let Some(task) = snapshot.iter().find(|task| task.id == task_id) {
            if let Err(e) = persist_repo_clone_path(db, task.repo_id, &task.local_path).await {
                tracing::warn!("Could not persist repository clone path: {}", e);
            }
        }
    }
    if let Err(e) = persist_clone_task_snapshot(db, &snapshot).await {
        tracing::warn!("Could not persist clone task completion: {}", e);
    }
}

async fn persist_clone_tasks(
    db: &Database,
    tasks: &Arc<Mutex<Vec<CloneTaskDto>>>,
) -> Result<(), String> {
    let snapshot = snapshot_clone_tasks(tasks);
    persist_clone_task_snapshot(db, &snapshot).await
}

async fn persist_clone_task_snapshot(
    db: &Database,
    snapshot: &[CloneTaskDto],
) -> Result<(), String> {
    let json = serde_json::to_string(snapshot).map_err(|e| e.to_string())?;
    db.set_setting(CLONE_TASKS_SETTING_KEY, &json)
        .await
        .map_err(|e| e.to_string())
}

async fn load_clone_tasks(db: &Database) -> Result<Vec<CloneTaskDto>, String> {
    let Some(json) = db
        .get_setting(CLONE_TASKS_SETTING_KEY)
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(Vec::new());
    };

    let mut tasks: Vec<CloneTaskDto> = serde_json::from_str(&json).unwrap_or_else(|e| {
        tracing::warn!("Could not parse persisted clone tasks: {}", e);
        Vec::new()
    });
    let now = now_secs();
    let mut changed = false;
    for task in &mut tasks {
        if task.status == CloneTaskStatus::Running {
            task.status = CloneTaskStatus::Failed;
            task.finished_at = Some(now);
            task.error = Some("Clone was interrupted before the app closed.".to_string());
            if !task.log.ends_with('\n') {
                task.log.push('\n');
            }
            task.log
                .push_str("Clone was interrupted before the app closed.\n");
            changed = true;
        }
    }
    if changed {
        persist_clone_task_snapshot(db, &tasks).await?;
    }
    Ok(tasks)
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn git_program() -> &'static str {
    if cfg!(target_os = "macos") {
        "/usr/bin/git"
    } else {
        "git"
    }
}

fn validate_repo_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() || name.contains('/') || name.contains('\\') {
        return Err(format!("Invalid repository name: {name}"));
    }
    Ok(())
}

fn repo_ssh_host_alias(repo: &Repository) -> String {
    let readable = sanitize_host_part(&repo.full_name.replace('/', "-"));
    format!("deploykeys-{}-{}", repo.id, readable)
}

fn sanitize_host_part(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for c in value.chars() {
        let next = if c.is_ascii_alphanumeric() {
            Some(c.to_ascii_lowercase())
        } else if matches!(c, '-' | '_' | '.') {
            Some('-')
        } else {
            None
        };

        if let Some(c) = next {
            if c == '-' {
                if !last_dash && !out.is_empty() {
                    out.push(c);
                }
                last_dash = true;
            } else {
                out.push(c);
                last_dash = false;
            }
        }
    }
    out.trim_matches('-').to_string()
}

// ---- App setup ------------------------------------------------------------

/// Entry point shared by the binary. Opens the database, registers state and
/// commands, and runs the Tauri event loop.
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Resolve the data dir first so we can both open the database and,
            // in development, point the file-backed credential store at it —
            // all before the first command can touch the keyring or DB.
            let (db, clone_tasks) = tauri::async_runtime::block_on(async {
                let data_dir = resolve_data_dir().await?;
                install_credential_backend(&data_dir);
                let db = open_database(&data_dir).await?;
                let clone_tasks =
                    load_clone_tasks(&db)
                        .await
                        .map_err(|e| -> Box<dyn std::error::Error> {
                            std::io::Error::other(e).into()
                        })?;
                Ok::<_, Box<dyn std::error::Error>>((db, clone_tasks))
            })?;
            app.manage(AppState::new(db, clone_tasks));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_session,
            get_language,
            set_language,
            get_page_size,
            set_page_size,
            get_active_connection,
            set_active_connection,
            sign_in_with_token,
            open_url,
            sign_out,
            list_repositories,
            sync_repositories,
            list_ssh_keys,
            create_ssh_key,
            delete_ssh_key,
            update_ssh_key,
            get_public_key_content,
            ssh_key_files_exist,
            bind_deploy_key,
            clone_repository,
            list_clone_tasks,
            clear_clone_tasks,
            connect_repository_remote,
            test_repository_remote,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Whether to store credentials in a plaintext file instead of the OS keyring.
///
/// Defaults to file-backed in debug builds (so `cargo tauri dev`'s unstable
/// ad-hoc signature does not trigger endless macOS keychain trust prompts) and
/// keyring in release. Override either way with
/// `DEPLOYKEYS_CREDENTIALS_BACKEND=file|keychain`.
fn use_file_credentials() -> bool {
    match std::env::var("DEPLOYKEYS_CREDENTIALS_BACKEND")
        .ok()
        .as_deref()
    {
        Some("file") => true,
        Some("keychain") => false,
        _ => cfg!(debug_assertions),
    }
}

/// Install the dev file-backed credential store when enabled. No-op otherwise
/// (the keyring crate's platform default — macOS Keychain — stays in effect).
fn install_credential_backend(data_dir: &std::path::Path) {
    if !use_file_credentials() {
        return;
    }
    let path = data_dir.join("dev_credentials.json");
    if let Err(e) = deploykeys_core::credentials::file_store::install(path.clone()) {
        tracing::error!("Could not install dev credential store: {}", e);
        return;
    }
    tracing::warn!(
        "Using INSECURE file-backed credential store (dev only). Tokens are stored \
         UNENCRYPTED at {}. Set DEPLOYKEYS_CREDENTIALS_BACKEND=keychain to use the OS keyring.",
        path.display()
    );
}

/// Resolve (and create) the application data directory under the OS data dir,
/// migrating pre-rename `deplock/` state on first launch so existing installs
/// keep their database and settings.
async fn resolve_data_dir() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let base = dirs::data_dir().ok_or("Could not determine the user data directory")?;
    let data_dir = base.join("deploykeys");

    // Pre-rename installs kept state under `deplock/`. Move the whole directory
    // over on first launch so existing databases and language prefs survive.
    let legacy_dir = base.join("deplock");
    if !data_dir.exists() && legacy_dir.exists() {
        if let Err(e) = tokio::fs::rename(&legacy_dir, &data_dir).await {
            tracing::warn!("Could not migrate legacy data directory: {}", e);
        } else {
            tracing::info!("Migrated data directory from deplock/ to deploykeys/");
            // The -wal/-shm sidecars must move with the DB file: an orphaned
            // WAL drops un-checkpointed pages and corrupts the database.
            for suffix in ["", "-wal", "-shm"] {
                let legacy = data_dir.join(format!("deplock.db{suffix}"));
                if legacy.exists() {
                    let _ =
                        tokio::fs::rename(&legacy, data_dir.join(format!("deploykeys.db{suffix}")))
                            .await;
                }
            }
        }
    }

    tokio::fs::create_dir_all(&data_dir).await?;
    Ok(data_dir)
}

/// Open (or create) the application database under `data_dir`, running migrations.
async fn open_database(data_dir: &std::path::Path) -> Result<Database, Box<dyn std::error::Error>> {
    let db = Database::new(&data_dir.join("deploykeys.db")).await?;
    db.run_migrations().await?;
    Ok(db)
}
