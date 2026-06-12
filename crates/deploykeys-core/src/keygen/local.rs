use crate::models::KeyAlgorithm;
use crate::{Error, Result};
use ssh_key::private::{KeypairData, RsaKeypair};
use ssh_key::{Algorithm, HashAlg, LineEnding, PrivateKey};
use std::path::{Path, PathBuf};

/// A generated SSH key pair with metadata
#[derive(Debug, Clone)]
pub struct KeyPair {
    pub algorithm: KeyAlgorithm,
    pub public_key: String,
    pub fingerprint: String,
    pub private_key_path: PathBuf,
}

/// Local SSH key generator
pub struct LocalKeyGenerator;

impl LocalKeyGenerator {
    /// Generate a new SSH key pair at the specified path
    ///
    /// # Security
    /// - On Unix, private keys are created with mode 0o600 (owner read/write only)
    /// - Public keys are created with mode 0o644 (world-readable)
    /// - Files are created atomically with correct permissions to prevent race conditions
    /// - If writing the public key fails, the private key file is removed
    ///
    /// # Errors
    /// - Returns `Error::Validation` if the parent directory does not exist
    /// - Returns `Error::AlreadyExists` if a key file already exists
    /// - Returns `Error::KeyGen` if key generation fails
    pub fn generate(algorithm: KeyAlgorithm, output_path: &Path, comment: &str) -> Result<KeyPair> {
        if let Some(parent) = output_path.parent() {
            if !parent.exists() {
                return Err(Error::Validation(format!(
                    "Parent directory does not exist: {}",
                    parent.display()
                )));
            }
        }

        let private_key = generate_private_key(&algorithm)?;
        let public_key = private_key.public_key();

        let private_openssh = private_key
            .to_openssh(LineEnding::LF)
            .map_err(|e| Error::KeyGen(format!("Failed to encode private key: {}", e)))?;

        let public_openssh = public_key
            .to_openssh()
            .map_err(|e| Error::KeyGen(format!("Failed to encode public key: {}", e)))?;

        let public_path = output_path.with_extension("pub");
        let public_line = format!("{} {}\n", public_openssh, comment);

        write_key_files(
            output_path,
            &public_path,
            private_openssh.as_bytes(),
            public_line.as_bytes(),
        )?;

        let fingerprint = public_key.fingerprint(HashAlg::Sha256).to_string();

        Ok(KeyPair {
            algorithm,
            public_key: format!("{} {}", public_openssh, comment),
            fingerprint,
            private_key_path: output_path.to_path_buf(),
        })
    }
}

fn generate_private_key(algorithm: &KeyAlgorithm) -> Result<PrivateKey> {
    let mut rng = rand::thread_rng();

    let rsa_bits = match algorithm {
        KeyAlgorithm::Rsa2048 => Some(2048),
        KeyAlgorithm::Rsa4096 => Some(4096),
        _ => None,
    };

    if let Some(bits) = rsa_bits {
        let keypair = RsaKeypair::random(&mut rng, bits)
            .map_err(|e| Error::KeyGen(format!("Failed to generate RSA-{} key: {}", bits, e)))?;
        return PrivateKey::new(KeypairData::Rsa(keypair), "")
            .map_err(|e| Error::KeyGen(format!("Failed to assemble RSA key: {}", e)));
    }

    let ssh_algorithm = match algorithm {
        KeyAlgorithm::Ed25519 => Algorithm::Ed25519,
        KeyAlgorithm::EcdsaP256 => Algorithm::Ecdsa {
            curve: ssh_key::EcdsaCurve::NistP256,
        },
        KeyAlgorithm::EcdsaP384 => Algorithm::Ecdsa {
            curve: ssh_key::EcdsaCurve::NistP384,
        },
        KeyAlgorithm::EcdsaP521 => Algorithm::Ecdsa {
            curve: ssh_key::EcdsaCurve::NistP521,
        },
        KeyAlgorithm::Rsa2048 | KeyAlgorithm::Rsa4096 => unreachable!("handled above"),
    };

    PrivateKey::random(&mut rng, ssh_algorithm)
        .map_err(|e| Error::KeyGen(format!("Failed to generate key: {}", e)))
}

#[cfg(unix)]
fn write_key_files(
    private_path: &Path,
    public_path: &Path,
    private_bytes: &[u8],
    public_bytes: &[u8],
) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let open_new = |path: &Path, mode: u32| {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    Error::AlreadyExists(format!("Key file already exists: {}", path.display()))
                } else {
                    Error::Io(e)
                }
            })
    };

    let mut private_file = open_new(private_path, 0o600)?;
    if let Err(e) = private_file.write_all(private_bytes) {
        let _ = std::fs::remove_file(private_path);
        return Err(Error::Io(e));
    }

    let public_result =
        open_new(public_path, 0o644).and_then(|mut f| f.write_all(public_bytes).map_err(Error::Io));
    if let Err(e) = public_result {
        // Do not leave a private key behind without its public half.
        let _ = std::fs::remove_file(private_path);
        return Err(e);
    }

    Ok(())
}

#[cfg(not(unix))]
fn write_key_files(
    private_path: &Path,
    public_path: &Path,
    private_bytes: &[u8],
    public_bytes: &[u8],
) -> Result<()> {
    if private_path.exists() || public_path.exists() {
        return Err(Error::AlreadyExists(format!(
            "Key file already exists: {}",
            private_path.display()
        )));
    }

    std::fs::write(private_path, private_bytes)?;
    if let Err(e) = std::fs::write(public_path, public_bytes) {
        let _ = std::fs::remove_file(private_path);
        return Err(Error::Io(e));
    }

    Ok(())
}
