//! Tauri backend for DeployKeys Desktop.
//!
//! Hosts the native side of the app: opens the database, exposes the IPC
//! command surface the Leptos webview calls, and bridges to `deploykeys-core`
//! (GitHub device flow, account persistence, settings). The webview itself is
//! the separate `deploykeys-ui` crate, built to wasm by Trunk.

use deploykeys_core::db::Database;
use deploykeys_core::github::{DeviceFlowClient, PollResult};
use deploykeys_core::services::AuthService;
use serde::Serialize;
use tauri::{Manager, State};
#[cfg(target_os = "macos")]
use tauri_plugin_decorum::WebviewWindowExt;

/// GitHub App client ID for the device flow (public information).
/// Override with the `DEPLOYKEYS_GITHUB_CLIENT_ID` environment variable.
const DEFAULT_GITHUB_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";

/// `app_settings` key under which the chosen UI language is stored.
const LANGUAGE_SETTING_KEY: &str = "language";

fn github_client_id() -> String {
    std::env::var("DEPLOYKEYS_GITHUB_CLIENT_ID")
        .unwrap_or_else(|_| DEFAULT_GITHUB_CLIENT_ID.to_string())
}

/// Shared native state, managed by Tauri and injected into every command.
struct AppState {
    db: Database,
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

#[derive(Serialize)]
struct DeviceCodeDto {
    device_code: String,
    user_code: String,
    verification_uri: String,
    interval: u64,
    expires_in: u64,
}

/// Result of a single token poll. Tagged so the UI can match on `status`.
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum PollDto {
    Pending,
    SlowDown,
    Authorized { account: AccountDto },
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

/// Begin a GitHub device flow: request a device + user code. The UI then shows
/// the code and polls `poll_github_auth` on the returned interval.
#[tauri::command]
async fn start_github_auth() -> Result<DeviceCodeDto, String> {
    let client = DeviceFlowClient::new(github_client_id()).map_err(|e| e.to_string())?;
    let code = client
        .request_device_code()
        .await
        .map_err(|e| e.to_string())?;
    Ok(DeviceCodeDto {
        device_code: code.device_code,
        user_code: code.user_code,
        verification_uri: code.verification_uri,
        interval: code.interval,
        expires_in: code.expires_in,
    })
}

/// Poll the token endpoint once. On authorization, completes the sign-in
/// (stores tokens in the keyring, persists the account) and returns the account.
#[tauri::command]
async fn poll_github_auth(
    state: State<'_, AppState>,
    device_code: String,
) -> Result<PollDto, String> {
    let client = DeviceFlowClient::new(github_client_id()).map_err(|e| e.to_string())?;
    match client
        .poll_for_token(&device_code)
        .await
        .map_err(|e| e.to_string())?
    {
        PollResult::Pending => Ok(PollDto::Pending),
        PollResult::SlowDown => Ok(PollDto::SlowDown),
        PollResult::Authorized(tokens) => {
            let service = AuthService::new(state.db.clone()).map_err(|e| e.to_string())?;
            let account = service
                .complete_device_flow(tokens)
                .await
                .map_err(|e| e.to_string())?;
            Ok(PollDto::Authorized {
                account: AccountDto {
                    login: account.login,
                    avatar_url: account.avatar_url,
                },
            })
        }
    }
}

/// Open a URL in the user's default browser. Failure is reported but rarely
/// matters — the UI still shows the code to copy manually.
#[tauri::command]
async fn open_url(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| e.to_string())
}

// ---- App setup ------------------------------------------------------------

/// Center the macOS traffic-light buttons in our custom header.
///
/// decorum's `set_traffic_lights_inset(x, y)` does not place buttons "y pixels
/// from the top": per its `traffic.rs` the button center ends up at
/// `y/2 + 4 + BUTTON_HEIGHT/2` (AppKit logical points — same unit as our CSS
/// px, independent of display scale). To center the lights in the header
/// (mid-line = HEADER_HEIGHT/2), solve for y:
///     HEADER_HEIGHT/2 = y/2 + 4 + BUTTON_HEIGHT/2
///     y = HEADER_HEIGHT - 8 - BUTTON_HEIGHT
/// then nudge the center up 1px (subtract 2 from y, since y is halved).
///
/// This must be re-applied on focus/resize: macOS resets the buttons to the
/// system inset whenever the window regains key or is resized, so a one-shot
/// call at startup gets overwritten. Keep `HEADER_HEIGHT` in sync with the UI
/// header; the lights re-center themselves when it changes.
#[cfg(target_os = "macos")]
fn center_traffic_lights(window: &tauri::WebviewWindow) {
    /// UI header height in logical px (matches the `h-14` header).
    const HEADER_HEIGHT: f32 = 56.0;
    /// macOS traffic-light button height (logical points).
    const BUTTON_HEIGHT: f32 = 12.0;
    let inset_y = HEADER_HEIGHT - 8.0 - BUTTON_HEIGHT - 2.0;
    let _ = window.set_traffic_lights_inset(16.0, inset_y);
}

/// Entry point shared by the binary. Opens the database, registers state and
/// commands, and runs the Tauri event loop.
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Note: we deliberately do NOT register `tauri_plugin_decorum::init()`. Its
    // `on_window_ready` installs a persistent traffic-light positioner hardcoded
    // to the plugin's default inset (PAD_Y = 16), which runs after `setup` and
    // overrides our custom inset — and re-pins to 16 on every resize. We only
    // want decorum's `set_traffic_lights_inset` trait method, which works
    // standalone, so we position the lights ourselves and re-apply on the window
    // events that would otherwise reset them.
    tauri::Builder::default()
        .setup(|app| {
            // Open the database on the Tauri async runtime before the first
            // command can run, then stash it in managed state.
            let db = tauri::async_runtime::block_on(init_database())?;
            app.manage(AppState { db });

            #[cfg(target_os = "macos")]
            {
                let window = app.get_webview_window("main").unwrap();
                center_traffic_lights(&window);

                // macOS resets the traffic-light position when the window gains
                // focus or resizes, so re-apply our inset on those events to keep
                // them centered in the header.
                let win = window.clone();
                window.on_window_event(move |event| {
                    if matches!(
                        event,
                        tauri::WindowEvent::Focused(true) | tauri::WindowEvent::Resized(_)
                    ) {
                        center_traffic_lights(&win);
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_session,
            get_language,
            set_language,
            start_github_auth,
            poll_github_auth,
            open_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Open (or create) the application database under the OS data directory,
/// running migrations. Includes the one-time migration of pre-rename
/// `deplock/` state so existing installs keep their database and settings.
async fn init_database() -> Result<Database, Box<dyn std::error::Error>> {
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

    let db = Database::new(&data_dir.join("deploykeys.db")).await?;
    db.run_migrations().await?;
    Ok(db)
}
