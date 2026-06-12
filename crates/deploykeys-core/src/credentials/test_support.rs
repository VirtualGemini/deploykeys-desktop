//! In-memory keyring backend for tests.
//!
//! `keyring`'s built-in mock store creates independent storage per `Entry`,
//! which breaks store-then-get flows that construct a new `Entry` per call.
//! This backend keys credentials by `(service, user)` in a process-wide map.

use keyring::credential::{Credential, CredentialApi, CredentialBuilderApi};
use keyring::Error as KeyringError;
use std::any::Any;
use std::collections::HashMap;
use std::sync::{Mutex, Once, OnceLock};

type Store = Mutex<HashMap<(String, String), Vec<u8>>>;

fn store() -> &'static Store {
    static STORE: OnceLock<Store> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug)]
struct InMemoryCredential {
    service: String,
    user: String,
}

impl InMemoryCredential {
    fn key(&self) -> (String, String) {
        (self.service.clone(), self.user.clone())
    }
}

impl CredentialApi for InMemoryCredential {
    fn set_password(&self, password: &str) -> keyring::Result<()> {
        self.set_secret(password.as_bytes())
    }

    fn set_secret(&self, secret: &[u8]) -> keyring::Result<()> {
        store()
            .lock()
            .expect("store lock")
            .insert(self.key(), secret.to_vec());
        Ok(())
    }

    fn get_password(&self) -> keyring::Result<String> {
        let bytes = self.get_secret()?;
        String::from_utf8(bytes).map_err(|e| KeyringError::BadEncoding(e.into_bytes()))
    }

    fn get_secret(&self) -> keyring::Result<Vec<u8>> {
        store()
            .lock()
            .expect("store lock")
            .get(&self.key())
            .cloned()
            .ok_or(KeyringError::NoEntry)
    }

    fn delete_credential(&self) -> keyring::Result<()> {
        store()
            .lock()
            .expect("store lock")
            .remove(&self.key())
            .map(|_| ())
            .ok_or(KeyringError::NoEntry)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Debug)]
struct InMemoryCredentialBuilder;

impl CredentialBuilderApi for InMemoryCredentialBuilder {
    fn build(
        &self,
        _target: Option<&str>,
        service: &str,
        user: &str,
    ) -> keyring::Result<Box<Credential>> {
        Ok(Box::new(InMemoryCredential {
            service: service.to_string(),
            user: user.to_string(),
        }))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Route all `keyring::Entry` operations in this process to the in-memory
/// store. Safe to call from every test; the swap happens once.
pub(crate) fn use_mock_keyring() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        keyring::set_default_credential_builder(Box::new(InMemoryCredentialBuilder));
    });
}
