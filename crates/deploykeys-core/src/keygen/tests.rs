use crate::keygen::local::LocalKeyGenerator;
use crate::models::KeyAlgorithm;
use crate::Error;
use tempfile::TempDir;

#[test]
fn ed25519_generation_writes_key_pair_with_permissions() {
    let temp_dir = TempDir::new().unwrap();
    let key_path = temp_dir.path().join("id_ed25519");

    let key_pair =
        LocalKeyGenerator::generate(KeyAlgorithm::Ed25519, &key_path, "test@example.com").unwrap();

    assert_eq!(key_pair.algorithm, KeyAlgorithm::Ed25519);
    assert!(key_pair.public_key.starts_with("ssh-ed25519"));
    assert!(key_pair.public_key.ends_with("test@example.com"));
    assert!(key_pair.fingerprint.starts_with("SHA256:"));
    assert!(key_path.exists());
    assert!(key_path.with_extension("pub").exists());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let private_meta = std::fs::metadata(&key_path).unwrap();
        let public_meta = std::fs::metadata(key_path.with_extension("pub")).unwrap();
        assert_eq!(private_meta.permissions().mode() & 0o777, 0o600);
        assert_eq!(public_meta.permissions().mode() & 0o777, 0o644);
    }
}

#[test]
fn ecdsa_p256_generation_uses_correct_key_type() {
    let temp_dir = TempDir::new().unwrap();
    let key_path = temp_dir.path().join("id_ecdsa");

    let key_pair =
        LocalKeyGenerator::generate(KeyAlgorithm::EcdsaP256, &key_path, "test@example.com")
            .unwrap();

    assert!(key_pair.public_key.starts_with("ecdsa-sha2-nistp256"));
}

#[test]
fn ecdsa_p384_generation_uses_correct_key_type() {
    let temp_dir = TempDir::new().unwrap();
    let key_path = temp_dir.path().join("id_ecdsa_p384");

    let key_pair =
        LocalKeyGenerator::generate(KeyAlgorithm::EcdsaP384, &key_path, "test@example.com")
            .unwrap();

    assert!(key_pair.public_key.starts_with("ecdsa-sha2-nistp384"));
}

#[test]
fn ecdsa_p521_generation_uses_correct_key_type() {
    let temp_dir = TempDir::new().unwrap();
    let key_path = temp_dir.path().join("id_ecdsa_p521");

    let key_pair =
        LocalKeyGenerator::generate(KeyAlgorithm::EcdsaP521, &key_path, "test@example.com")
            .unwrap();

    assert!(key_pair.public_key.starts_with("ecdsa-sha2-nistp521"));
}

#[test]
#[ignore = "RSA key generation is slow in debug builds; run with --ignored"]
fn rsa2048_generation_respects_bit_size() {
    let temp_dir = TempDir::new().unwrap();
    let key_path = temp_dir.path().join("id_rsa");

    let key_pair =
        LocalKeyGenerator::generate(KeyAlgorithm::Rsa2048, &key_path, "test@example.com").unwrap();

    let public = ssh_key::PublicKey::from_openssh(&key_pair.public_key).unwrap();
    let rsa = public.key_data().rsa().expect("expected an RSA key");
    let n_bytes = rsa.n.as_positive_bytes().unwrap_or(rsa.n.as_bytes());
    assert_eq!(n_bytes.len() * 8, 2048, "selected bit size must be honored");
}

#[test]
fn second_generation_at_same_path_fails() {
    let temp_dir = TempDir::new().unwrap();
    let key_path = temp_dir.path().join("id_ed25519");

    LocalKeyGenerator::generate(KeyAlgorithm::Ed25519, &key_path, "test@example.com").unwrap();

    let result = LocalKeyGenerator::generate(KeyAlgorithm::Ed25519, &key_path, "test@example.com");

    assert!(matches!(result.unwrap_err(), Error::AlreadyExists(_)));
}

#[test]
fn missing_parent_directory_is_rejected() {
    let key_path = std::path::PathBuf::from("/nonexistent/dir/id_ed25519");

    let result = LocalKeyGenerator::generate(KeyAlgorithm::Ed25519, &key_path, "test@example.com");

    assert!(matches!(result.unwrap_err(), Error::Validation(_)));
}

#[test]
fn public_key_write_failure_cleans_up_private_key() {
    let temp_dir = TempDir::new().unwrap();
    let key_path = temp_dir.path().join("id_ed25519");

    // Pre-existing .pub file forces the second write to fail after the
    // private key has been created.
    std::fs::write(key_path.with_extension("pub"), "stale").unwrap();

    let result = LocalKeyGenerator::generate(KeyAlgorithm::Ed25519, &key_path, "test@example.com");

    assert!(result.is_err());
    assert!(
        !key_path.exists(),
        "private key must not be left behind when the public key write fails"
    );
}
