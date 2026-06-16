//! Tauri backend for DeployKeys Desktop.
//!
//! Hosts the native side of the app: opens the database, exposes the IPC
//! command surface the Leptos webview calls, and bridges to `deploykeys-core`
//! (PAT sign-in, repo sync, account persistence, settings). The webview itself
//! is the separate `deploykeys-ui` crate, built to wasm by Trunk.

use deploykeys_core::credentials::CredentialStore;
use deploykeys_core::db::Database;
use deploykeys_core::models::{Account, KeyAlgorithm};
use deploykeys_core::progress::{OperationId, ProgressReporter};
use deploykeys_core::services::{AuthService, RepoSyncService, SshKeyService};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

const PROGRESS_EVENT: &str = "progress";

/// `app_settings` key under which the chosen UI language is stored.
const LANGUAGE_SETTING_KEY: &str = "language";
/// `app_settings` key under which the repository-list page size is stored.
const PAGE_SIZE_SETTING_KEY: &str = "repos_page_size";

/// Shared native state, managed by Tauri and injected into every command.
struct AppState {
    db: Database,
    /// The active session's GitHub access token, cached in memory so repeated
    /// operations (e.g. repo sync) don't re-read the OS keyring — each keyring
    /// read can trigger a macOS keychain trust prompt. Populated on sign-in and
    /// lazily on first read; cleared on sign-out.
    token: std::sync::Mutex<Option<String>>,
}

impl AppState {
    fn new(db: Database) -> Self {
        Self {
            db,
            token: std::sync::Mutex::new(None),
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
        .setup(|app| {
            // Resolve the data dir first so we can both open the database and,
            // in development, point the file-backed credential store at it —
            // all before the first command can touch the keyring or DB.
            let db = tauri::async_runtime::block_on(async {
                let data_dir = resolve_data_dir().await?;
                install_credential_backend(&data_dir);
                open_database(&data_dir).await
            })?;
            app.manage(AppState::new(db));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_session,
            get_language,
            set_language,
            get_page_size,
            set_page_size,
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
