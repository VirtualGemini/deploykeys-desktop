//! File-backed credential store for development.
//!
//! `cargo tauri dev` produces an ad-hoc-signed binary whose code-signing
//! identity changes on every rebuild, so the macOS keychain treats each access
//! as coming from a new app and re-prompts for trust. In development we instead
//! route `keyring` to a plaintext JSON file: no prompts, and tokens survive
//! restarts.
//!
//! NEVER enable this for release builds — secrets are stored unencrypted.

use crate::Result;
use keyring::credential::{Credential, CredentialApi, CredentialBuilderApi};
use keyring::Error as KeyringError;
use std::any::Any;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Process-wide store, keyed by `service\u{1f}user` (the same identifiers the
/// real keyring uses), with secrets persisted to `path` as JSON.
struct State {
    path: PathBuf,
    entries: HashMap<String, Vec<u8>>,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();

fn entry_key(service: &str, user: &str) -> String {
    format!("{service}\u{1f}{user}")
}

fn state() -> &'static Mutex<State> {
    STATE.get().expect("file credential store not installed")
}

/// Install the file-backed store as the process-wide keyring backend, loading
/// any previously-persisted secrets. Idempotent; safe to call once at startup.
pub fn install(path: PathBuf) -> Result<()> {
    if STATE.get().is_some() {
        return Ok(());
    }
    let entries = load(&path).unwrap_or_default();
    // If another thread won the race, the value we built is dropped — harmless.
    let _ = STATE.set(Mutex::new(State { path, entries }));
    keyring::set_default_credential_builder(Box::new(FileCredentialBuilder));
    Ok(())
}

fn load(path: &Path) -> Option<HashMap<String, Vec<u8>>> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn persist(state: &State) {
    if let Ok(bytes) = serde_json::to_vec_pretty(&state.entries) {
        if let Err(e) = std::fs::write(&state.path, bytes) {
            tracing::warn!("Could not persist dev credentials: {}", e);
        }
    }
}

#[derive(Debug)]
struct FileCredential {
    service: String,
    user: String,
}

impl CredentialApi for FileCredential {
    fn set_password(&self, password: &str) -> keyring::Result<()> {
        self.set_secret(password.as_bytes())
    }

    fn set_secret(&self, secret: &[u8]) -> keyring::Result<()> {
        let mut s = state().lock().expect("file store lock");
        s.entries
            .insert(entry_key(&self.service, &self.user), secret.to_vec());
        persist(&s);
        Ok(())
    }

    fn get_password(&self) -> keyring::Result<String> {
        let bytes = self.get_secret()?;
        String::from_utf8(bytes).map_err(|e| KeyringError::BadEncoding(e.into_bytes()))
    }

    fn get_secret(&self) -> keyring::Result<Vec<u8>> {
        let s = state().lock().expect("file store lock");
        s.entries
            .get(&entry_key(&self.service, &self.user))
            .cloned()
            .ok_or(KeyringError::NoEntry)
    }

    fn delete_credential(&self) -> keyring::Result<()> {
        let mut s = state().lock().expect("file store lock");
        if s.entries
            .remove(&entry_key(&self.service, &self.user))
            .is_some()
        {
            persist(&s);
            Ok(())
        } else {
            Err(KeyringError::NoEntry)
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Debug)]
struct FileCredentialBuilder;

impl CredentialBuilderApi for FileCredentialBuilder {
    fn build(
        &self,
        _target: Option<&str>,
        service: &str,
        user: &str,
    ) -> keyring::Result<Box<Credential>> {
        Ok(Box::new(FileCredential {
            service: service.to_string(),
            user: user.to_string(),
        }))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
