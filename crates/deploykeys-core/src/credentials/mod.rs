use crate::Result;
use keyring::Entry;

pub mod file_store;

/// Manages secure credential storage using the system keyring
///
/// Credentials are stored in:
/// - macOS: Keychain
/// - Linux: Secret Service / libsecret
///
/// # Security
/// - Never stores credentials in SQLite or plain files
/// - Returns reference keys that identify credentials in the system keyring
pub struct CredentialStore;

impl CredentialStore {
    const SERVICE_NAME: &'static str = "com.deploykeys.desktop";

    /// Service name used before the project was renamed to DeployKeys.
    /// Retained only so [`Self::migrate_legacy_entry`] can move old
    /// credentials into the current namespace on first launch.
    const LEGACY_SERVICE_NAME: &'static str = "com.deplock.desktop";

    /// Move one credential from the legacy (`com.deplock.desktop`) service
    /// namespace into the current one.
    ///
    /// The reference keys themselves are unchanged by the rename — only the
    /// service namespace differs — so migration is keyed by `ref_key`.
    ///
    /// Best-effort and idempotent: returns `Ok(false)` when the current entry
    /// already exists or the legacy entry is absent; `Ok(true)` when a secret
    /// was actually moved. On success the legacy entry is deleted.
    pub fn migrate_legacy_entry(ref_key: &str) -> Result<bool> {
        let current = Entry::new(Self::SERVICE_NAME, ref_key)?;
        if current.get_password().is_ok() {
            return Ok(false);
        }

        let legacy = Entry::new(Self::LEGACY_SERVICE_NAME, ref_key)?;
        match legacy.get_password() {
            Ok(secret) => {
                current.set_password(&secret)?;
                // The secret now lives under the new namespace; drop the old
                // copy. A failed delete is non-fatal (the new copy is what we
                // read going forward).
                let _ = legacy.delete_credential();
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }

    /// Store a GitHub token and return the reference key
    pub fn store_token(account_login: &str, token: &str) -> Result<String> {
        validate_login(account_login)?;
        validate_secret(token, "token")?;

        let key = format!("github_token_{}", account_login);
        let entry = Entry::new(Self::SERVICE_NAME, &key)?;
        entry.set_password(token)?;
        Ok(key)
    }

    /// Retrieve a GitHub token by reference key
    pub fn get_token(token_ref: &str) -> Result<String> {
        let entry = Entry::new(Self::SERVICE_NAME, token_ref)?;
        Ok(entry.get_password()?)
    }

    /// Delete a GitHub token by reference key
    pub fn delete_token(token_ref: &str) -> Result<()> {
        let entry = Entry::new(Self::SERVICE_NAME, token_ref)?;
        Ok(entry.delete_credential()?)
    }

    /// Store a refresh token and return the reference key
    pub fn store_refresh_token(account_login: &str, token: &str) -> Result<String> {
        validate_login(account_login)?;
        validate_secret(token, "refresh token")?;

        let key = format!("github_refresh_token_{}", account_login);
        let entry = Entry::new(Self::SERVICE_NAME, &key)?;
        entry.set_password(token)?;
        Ok(key)
    }

    /// Retrieve a refresh token by reference key
    pub fn get_refresh_token(token_ref: &str) -> Result<String> {
        let entry = Entry::new(Self::SERVICE_NAME, token_ref)?;
        Ok(entry.get_password()?)
    }

    /// Store SSH password for a remote target
    pub fn store_ssh_password(target_id: i64, password: &str) -> Result<String> {
        validate_target_id(target_id)?;
        validate_secret(password, "password")?;

        let key = format!("ssh_password_target_{}", target_id);
        Self::store_ssh_password_ref(&key, password)
    }

    /// Store SSH password under an explicit reference key.
    pub fn store_ssh_password_ref(ref_key: &str, password: &str) -> Result<String> {
        validate_ref_key(ref_key)?;
        validate_secret(password, "password")?;

        let key = ref_key.to_string();
        let entry = Entry::new(Self::SERVICE_NAME, &key)?;
        entry.set_password(password)?;
        Ok(key)
    }

    /// Retrieve SSH password by reference key
    pub fn get_ssh_password(password_ref: &str) -> Result<String> {
        let entry = Entry::new(Self::SERVICE_NAME, password_ref)?;
        Ok(entry.get_password()?)
    }

    /// Store SSH key passphrase for a remote target
    pub fn store_ssh_passphrase(target_id: i64, passphrase: &str) -> Result<String> {
        validate_target_id(target_id)?;
        validate_secret(passphrase, "passphrase")?;

        let key = format!("ssh_passphrase_target_{}", target_id);
        let entry = Entry::new(Self::SERVICE_NAME, &key)?;
        entry.set_password(passphrase)?;
        Ok(key)
    }

    /// Retrieve SSH key passphrase by reference key
    pub fn get_ssh_passphrase(passphrase_ref: &str) -> Result<String> {
        let entry = Entry::new(Self::SERVICE_NAME, passphrase_ref)?;
        Ok(entry.get_password()?)
    }

    /// Delete a credential by reference key
    pub fn delete_credential(credential_ref: &str) -> Result<()> {
        let entry = Entry::new(Self::SERVICE_NAME, credential_ref)?;
        Ok(entry.delete_credential()?)
    }
}

fn validate_login(account_login: &str) -> Result<()> {
    if account_login.is_empty() {
        return Err(crate::Error::Validation(
            "account_login cannot be empty".to_string(),
        ));
    }
    Ok(())
}

fn validate_secret(value: &str, what: &str) -> Result<()> {
    if value.is_empty() {
        return Err(crate::Error::Validation(format!(
            "{} cannot be empty",
            what
        )));
    }
    Ok(())
}

fn validate_target_id(target_id: i64) -> Result<()> {
    if target_id <= 0 {
        return Err(crate::Error::Validation(
            "target_id must be positive".to_string(),
        ));
    }
    Ok(())
}

fn validate_ref_key(ref_key: &str) -> Result<()> {
    if ref_key.is_empty()
        || !ref_key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(crate::Error::Validation(
            "credential reference key is invalid".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod tests {
    use super::test_support::use_mock_keyring;
    use super::*;

    #[test]
    fn test_token_storage_roundtrip() {
        use_mock_keyring();

        let token_ref = CredentialStore::store_token("test_user", "ghu_test_token_123").unwrap();
        assert_eq!(token_ref, "github_token_test_user");

        let retrieved = CredentialStore::get_token(&token_ref).unwrap();
        assert_eq!(retrieved, "ghu_test_token_123");

        CredentialStore::delete_token(&token_ref).unwrap();
        assert!(CredentialStore::get_token(&token_ref).is_err());
    }

    #[test]
    fn test_refresh_token_roundtrip() {
        use_mock_keyring();

        let token_ref =
            CredentialStore::store_refresh_token("test_user", "ghr_refresh_123").unwrap();
        assert_eq!(token_ref, "github_refresh_token_test_user");

        let retrieved = CredentialStore::get_refresh_token(&token_ref).unwrap();
        assert_eq!(retrieved, "ghr_refresh_123");

        CredentialStore::delete_credential(&token_ref).unwrap();
    }

    #[test]
    fn test_ssh_password_roundtrip() {
        use_mock_keyring();

        let password_ref = CredentialStore::store_ssh_password(42, "hunter2").unwrap();
        assert_eq!(password_ref, "ssh_password_target_42");

        let retrieved = CredentialStore::get_ssh_password(&password_ref).unwrap();
        assert_eq!(retrieved, "hunter2");

        CredentialStore::delete_credential(&password_ref).unwrap();
    }

    #[test]
    fn test_ssh_passphrase_roundtrip() {
        use_mock_keyring();

        let passphrase_ref = CredentialStore::store_ssh_passphrase(7, "phrase").unwrap();
        assert_eq!(passphrase_ref, "ssh_passphrase_target_7");

        let retrieved = CredentialStore::get_ssh_passphrase(&passphrase_ref).unwrap();
        assert_eq!(retrieved, "phrase");

        CredentialStore::delete_credential(&passphrase_ref).unwrap();
    }
}

#[cfg(test)]
mod validation_tests;
