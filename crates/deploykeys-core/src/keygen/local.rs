use crate::models::KeyAlgorithm;
use crate::{Error, Result};
use ssh_key::{Algorithm, HashAlg, LineEnding, PrivateKey};
use std::path::{Path, PathBuf};
use std::process::Command;

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
    /// - Rust-generated key files are created atomically with correct permissions
    ///   to prevent race conditions
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

        if let Some(bits) = rsa_bits(&algorithm) {
            return generate_rsa_key_with_ssh_keygen(algorithm, bits, output_path, comment);
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

fn rsa_bits(algorithm: &KeyAlgorithm) -> Option<u32> {
    match algorithm {
        KeyAlgorithm::Rsa2048 => Some(2048),
        KeyAlgorithm::Rsa4096 => Some(4096),
        _ => None,
    }
}

fn generate_rsa_key_with_ssh_keygen(
    algorithm: KeyAlgorithm,
    bits: u32,
    output_path: &Path,
    comment: &str,
) -> Result<KeyPair> {
    let public_path = output_path.with_extension("pub");
    let ssh_keygen_public_path = appended_pub_path(output_path);

    if output_path.exists() || public_path.exists() || ssh_keygen_public_path.exists() {
        return Err(Error::AlreadyExists(format!(
            "Key file already exists: {}",
            output_path.display()
        )));
    }

    let output = Command::new("ssh-keygen")
        .arg("-q")
        .arg("-t")
        .arg("rsa")
        .arg("-b")
        .arg(bits.to_string())
        .arg("-N")
        .arg("")
        .arg("-C")
        .arg(comment)
        .arg("-f")
        .arg(output_path)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::KeyGen("ssh-keygen is required for RSA key generation".to_string())
            } else {
                Error::Io(e)
            }
        })?;

    if !output.status.success() {
        cleanup_key_files(output_path, &public_path, &ssh_keygen_public_path);
        return Err(Error::KeyGen(command_failure_message(
            "ssh-keygen failed to generate RSA key",
            &output,
        )));
    }

    if ssh_keygen_public_path != public_path {
        std::fs::rename(&ssh_keygen_public_path, &public_path).map_err(|e| {
            cleanup_key_files(output_path, &public_path, &ssh_keygen_public_path);
            Error::Io(e)
        })?;
    }

    let public_key = std::fs::read_to_string(&public_path)
        .map_err(Error::Io)?
        .trim_end()
        .to_string();
    let fingerprint = fingerprint_with_ssh_keygen(&public_path)?;

    Ok(KeyPair {
        algorithm,
        public_key,
        fingerprint,
        private_key_path: output_path.to_path_buf(),
    })
}

fn generate_private_key(algorithm: &KeyAlgorithm) -> Result<PrivateKey> {
    let mut rng = rand::thread_rng();

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
        KeyAlgorithm::Rsa2048 | KeyAlgorithm::Rsa4096 => {
            return Err(Error::KeyGen(
                "RSA key generation is handled by ssh-keygen".to_string(),
            ));
        }
    };

    PrivateKey::random(&mut rng, ssh_algorithm)
        .map_err(|e| Error::KeyGen(format!("Failed to generate key: {}", e)))
}

fn fingerprint_with_ssh_keygen(public_path: &Path) -> Result<String> {
    let output = Command::new("ssh-keygen")
        .arg("-l")
        .arg("-E")
        .arg("sha256")
        .arg("-f")
        .arg(public_path)
        .output()
        .map_err(Error::Io)?;

    if !output.status.success() {
        return Err(Error::KeyGen(command_failure_message(
            "ssh-keygen failed to fingerprint RSA key",
            &output,
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .split_whitespace()
        .nth(1)
        .map(str::to_string)
        .ok_or_else(|| Error::KeyGen("ssh-keygen returned an invalid fingerprint".to_string()))
}

fn appended_pub_path(path: &Path) -> PathBuf {
    let mut public_path = path.as_os_str().to_os_string();
    public_path.push(".pub");
    PathBuf::from(public_path)
}

fn cleanup_key_files(private_path: &Path, public_path: &Path, ssh_keygen_public_path: &Path) {
    let _ = std::fs::remove_file(private_path);
    let _ = std::fs::remove_file(public_path);
    if ssh_keygen_public_path != public_path {
        let _ = std::fs::remove_file(ssh_keygen_public_path);
    }
}

fn command_failure_message(context: &str, output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    if !stderr.is_empty() {
        return format!("{}: {}", context, stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stdout = stdout.trim();
    if !stdout.is_empty() {
        return format!("{}: {}", context, stdout);
    }

    format!("{}: {}", context, output.status)
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
