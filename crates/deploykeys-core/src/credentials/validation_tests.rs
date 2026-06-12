use super::test_support::use_mock_keyring;
use super::CredentialStore;
use crate::Error;

#[test]
fn empty_inputs_are_rejected() {
    use_mock_keyring();

    assert!(matches!(
        CredentialStore::store_token("", "token").unwrap_err(),
        Error::Validation(_)
    ));
    assert!(matches!(
        CredentialStore::store_token("user", "").unwrap_err(),
        Error::Validation(_)
    ));
    assert!(matches!(
        CredentialStore::store_refresh_token("", "token").unwrap_err(),
        Error::Validation(_)
    ));
    assert!(matches!(
        CredentialStore::store_refresh_token("user", "").unwrap_err(),
        Error::Validation(_)
    ));
    assert!(matches!(
        CredentialStore::store_ssh_passphrase(1, "").unwrap_err(),
        Error::Validation(_)
    ));
}

#[test]
fn non_positive_target_ids_are_rejected() {
    use_mock_keyring();

    assert!(matches!(
        CredentialStore::store_ssh_password(0, "password").unwrap_err(),
        Error::Validation(_)
    ));
    assert!(matches!(
        CredentialStore::store_ssh_password(-1, "password").unwrap_err(),
        Error::Validation(_)
    ));
    assert!(matches!(
        CredentialStore::store_ssh_passphrase(0, "phrase").unwrap_err(),
        Error::Validation(_)
    ));
}

#[test]
fn token_key_format_is_stable() {
    use_mock_keyring();

    let key = CredentialStore::store_token("testuser", "test_token_12345").unwrap();
    assert_eq!(key, "github_token_testuser");
    CredentialStore::delete_token(&key).unwrap();
}

#[test]
fn ssh_password_key_format_is_stable() {
    use_mock_keyring();

    let key = CredentialStore::store_ssh_password(42, "test_password").unwrap();
    assert_eq!(key, "ssh_password_target_42");
    CredentialStore::delete_credential(&key).unwrap();
}

#[test]
fn missing_credential_returns_error() {
    use_mock_keyring();

    assert!(CredentialStore::get_token("github_token_never_stored").is_err());
    assert!(CredentialStore::delete_credential("github_token_never_stored").is_err());
}
