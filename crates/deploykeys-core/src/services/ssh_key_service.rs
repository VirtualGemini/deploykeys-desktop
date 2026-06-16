use crate::{
    db::Database,
    keygen::local::LocalKeyGenerator,
    models::{KeyAlgorithm, SshKey},
    Error, Result,
};
use chrono::Utc;
use std::path::{Path, PathBuf};

/// Alias shown for the Phase-1 local machine in the `targets` table. SSH keys
/// created in Phase 1 are associated with this row so the connection layer can
/// resolve which machine (and where on that machine) a key lives on.
const LOCAL_TARGET_ALIAS: &str = "Local Machine";

/// SSH key management service for standalone local keys.
pub struct SshKeyService {
    db: Database,
}

impl SshKeyService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Generate a new SSH key pair in an isolated directory.
    ///
    /// Keys are stored in `~/.ssh/deploykeys/<directory>/id_<algorithm>`.
    /// The `directory` must be ASCII-safe (no spaces, only alphanumeric + dash/underscore).
    ///
    /// The key is associated with the Phase-1 local target, which is created on
    /// demand if it does not yet exist (so the `ssh_keys.target_id` FK always
    /// resolves). The key material itself is immutable; to edit the editable
    /// fields (directory, remark) after creation, use
    /// [`SshKeyService::update_key`].
    pub async fn create_key(
        &self,
        directory: String,
        algorithm: KeyAlgorithm,
        comment: String,
        remark: String,
    ) -> Result<SshKey> {
        let target_id = self.ensure_local_target().await?;
        let directory = directory.trim().to_string();
        // Validate directory is ASCII-safe
        if !directory
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(Error::Validation(
                "Directory must contain only ASCII letters, digits, hyphens, and underscores"
                    .into(),
            ));
        }
        if directory.is_empty() {
            return Err(Error::Validation("Directory is required".into()));
        }

        // The comment (conventionally an email) identifies the key's owner. It is
        // appended verbatim to the public key line, so it must not contain
        // whitespace — an embedded space would split the key line.
        let comment = comment.trim().to_string();
        if comment.is_empty() {
            return Err(Error::Validation("Comment/email is required".into()));
        }
        if comment.chars().any(char::is_whitespace) {
            return Err(Error::Validation(
                "Comment must not contain whitespace (use an email address)".into(),
            ));
        }

        // Check for duplicate directory
        if self
            .db
            .ssh_keys()
            .find_by_directory(&directory)
            .await?
            .is_some()
        {
            return Err(Error::AlreadyExists(format!(
                "SSH key with directory '{}' already exists",
                directory
            )));
        }

        // Resolve base directory: ~/.ssh/deploykeys/<directory>/
        let base_dir = resolve_ssh_keys_base_dir()?;
        let key_dir = base_dir.join(&directory);
        tokio::fs::create_dir_all(&key_dir).await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&key_dir, std::fs::Permissions::from_mode(0o700)).await?;
        }

        // Key file path: ~/.ssh/deploykeys/<directory>/id_<algorithm>
        let key_filename = format!("id_{}", algorithm.to_string().to_lowercase());
        let private_key_path = key_dir.join(&key_filename);

        // Generate key pair (CPU-bound, run on blocking thread). The comment
        // (typically an email) is embedded as the identity at the end of the
        // public key line.
        let gen_algorithm = algorithm.clone();
        let gen_path = private_key_path.clone();
        let gen_comment = comment.clone();
        let key_pair = tokio::task::spawn_blocking(move || {
            LocalKeyGenerator::generate(gen_algorithm, &gen_path, &gen_comment)
        })
        .await
        .map_err(|e| Error::Other(format!("Key generation task failed: {}", e)))??;

        let public_key_path = private_key_path.with_extension("pub");

        let ssh_key = SshKey {
            id: 0,
            directory,
            algorithm,
            public_key: key_pair.public_key,
            public_key_fingerprint: key_pair.fingerprint,
            private_key_path: private_key_path.to_string_lossy().to_string(),
            public_key_path: public_key_path.to_string_lossy().to_string(),
            comment,
            remark: remark.trim().to_string(),
            target_id,
            created_at: Utc::now(),
        };

        let id = self.db.ssh_keys().create(&ssh_key).await?;
        Ok(SshKey { id, ..ssh_key })
    }

    /// List all SSH keys for a given target.
    pub async fn list_keys(&self, target_id: i64) -> Result<Vec<SshKey>> {
        self.db.ssh_keys().list_by_target(target_id).await
    }

    /// List all SSH keys (used when target filtering is not needed).
    pub async fn list_all_keys(&self) -> Result<Vec<SshKey>> {
        self.db.ssh_keys().list_all().await
    }

    /// Get a single SSH key by ID.
    pub async fn get_key(&self, id: i64) -> Result<Option<SshKey>> {
        self.db.ssh_keys().find_by_id(id).await
    }

    /// Read the public key file content.
    pub async fn read_public_key(&self, id: i64) -> Result<String> {
        let key = self
            .db
            .ssh_keys()
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::NotFound("SSH key not found".into()))?;

        self.ensure_key_files_exist(&key).await?;

        let content = tokio::fs::read_to_string(&key.public_key_path)
            .await
            .map_err(|e| Error::Io(e))?;

        Ok(content)
    }

    /// Delete an SSH key and its files.
    pub async fn delete_key(&self, id: i64) -> Result<()> {
        let key = self
            .db
            .ssh_keys()
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::NotFound("SSH key not found".into()))?;

        // Delete key files
        let _ = tokio::fs::remove_file(&key.private_key_path).await;
        let _ = tokio::fs::remove_file(&key.public_key_path).await;

        // Try to remove the parent directory if it's empty
        if let Some(parent) = Path::new(&key.private_key_path).parent() {
            let _ = tokio::fs::remove_dir(parent).await;
        }

        // Delete from database
        self.db.ssh_keys().delete(id).await?;

        Ok(())
    }

    /// Return whether the key's directory and expected key files still exist.
    pub async fn key_files_exist(&self, id: i64) -> Result<bool> {
        let key = self
            .db
            .ssh_keys()
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::NotFound("SSH key not found".into()))?;

        Ok(key_files_exist(&key).await)
    }

    /// Update the editable fields of an SSH key: its directory and its remark.
    ///
    /// The key material (and the `comment`/email embedded in the public key line)
    /// is immutable; only the directory name and the free-form remark can change.
    /// Changing the directory renames the containing folder on disk and rewrites
    /// the stored file paths to match. Changing only the remark touches no files.
    pub async fn update_key(&self, id: i64, directory: &str, remark: &str) -> Result<SshKey> {
        let new_directory = directory.trim();
        if new_directory.is_empty() {
            return Err(Error::Validation("Directory cannot be empty".into()));
        }
        if !new_directory
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(Error::Validation(
                "Directory must contain only ASCII letters, digits, hyphens, and underscores"
                    .into(),
            ));
        }
        let new_remark = remark.trim().to_string();

        let key = self
            .db
            .ssh_keys()
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::NotFound("SSH key not found".into()))?;

        self.ensure_key_files_exist(&key).await?;

        // If the directory is unchanged, there is nothing to move on disk — just
        // persist the remark and the (unchanged) paths.
        if key.directory == new_directory {
            self.db
                .ssh_keys()
                .update_key(
                    id,
                    new_directory,
                    &new_remark,
                    &key.private_key_path,
                    &key.public_key_path,
                )
                .await?;

            return Ok(SshKey {
                remark: new_remark,
                ..key
            });
        }

        if self
            .db
            .ssh_keys()
            .find_by_directory(new_directory)
            .await?
            .is_some()
        {
            return Err(Error::AlreadyExists(format!(
                "SSH key with directory '{}' already exists",
                new_directory
            )));
        }

        let old_dir = Path::new(&key.private_key_path)
            .parent()
            .ok_or_else(|| Error::Validation("SSH key path has no parent directory".into()))?;
        let new_dir = old_dir
            .parent()
            .ok_or_else(|| Error::Validation("SSH key directory has no parent directory".into()))?
            .join(new_directory);

        if new_dir.exists() {
            return Err(Error::AlreadyExists(format!(
                "Directory already exists: {}",
                new_dir.display()
            )));
        }

        tokio::fs::rename(old_dir, &new_dir).await?;

        let private_file_name = Path::new(&key.private_key_path)
            .file_name()
            .ok_or_else(|| Error::Validation("SSH private key path has no file name".into()))?;
        let public_file_name = Path::new(&key.public_key_path)
            .file_name()
            .ok_or_else(|| Error::Validation("SSH public key path has no file name".into()))?;
        let private_key_path = new_dir.join(private_file_name);
        let public_key_path = new_dir.join(public_file_name);

        if let Err(e) = self
            .db
            .ssh_keys()
            .update_key(
                id,
                new_directory,
                &new_remark,
                &private_key_path.to_string_lossy(),
                &public_key_path.to_string_lossy(),
            )
            .await
        {
            // Roll back the on-disk rename so the DB and filesystem stay in sync.
            let _ = tokio::fs::rename(&new_dir, old_dir).await;
            return Err(e);
        }

        Ok(SshKey {
            directory: new_directory.to_string(),
            remark: new_remark,
            private_key_path: private_key_path.to_string_lossy().to_string(),
            public_key_path: public_key_path.to_string_lossy().to_string(),
            ..key
        })
    }

    /// Ensure the Phase-1 local target row exists and return its id.
    ///
    /// `ssh_keys.target_id` is a FK into `targets(id)`; without a row here the
    /// insert fails with `FOREIGN KEY constraint failed`. We look the target up
    /// by its stable alias, and create it on first use with `key_base_dir`
    /// pointing at the same directory keys are actually written to.
    async fn ensure_local_target(&self) -> Result<i64> {
        if let Some(existing) = self.db.targets().find_by_alias(LOCAL_TARGET_ALIAS).await? {
            return Ok(existing.id);
        }

        let key_base_dir = resolve_ssh_keys_base_dir()?;
        let target = crate::models::Target {
            id: 0,
            target_type: crate::models::TargetType::Local,
            alias: LOCAL_TARGET_ALIAS.to_string(),
            os: detect_os(),
            host: None,
            port: None,
            username: None,
            auth_method: None,
            auth_ref: None,
            key_base_dir: key_base_dir.to_string_lossy().to_string(),
            status: crate::models::TargetStatus::Active,
            host_key_fingerprint: None,
            created_at: Utc::now(),
            last_checked_at: Some(Utc::now()),
        };
        let id = self.db.targets().create(&target).await?;
        Ok(id)
    }
}

fn detect_os() -> crate::models::OsType {
    if cfg!(target_os = "macos") {
        crate::models::OsType::MacOs
    } else if cfg!(target_os = "linux") {
        crate::models::OsType::Linux
    } else {
        crate::models::OsType::Unknown
    }
}

fn resolve_ssh_keys_base_dir() -> Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| Error::Other("Could not find home directory".into()))?;
    Ok(home.join(".ssh").join("deploykeys"))
}

async fn key_files_exist(key: &SshKey) -> bool {
    let private_path = Path::new(&key.private_key_path);
    let public_path = Path::new(&key.public_key_path);

    private_path
        .parent()
        .is_some_and(|dir| dir.exists() && dir.is_dir())
        && private_path.exists()
        && public_path.exists()
}

impl SshKeyService {
    async fn ensure_key_files_exist(&self, key: &SshKey) -> Result<()> {
        if key_files_exist(key).await {
            return Ok(());
        }

        Err(Error::NotFound(
            "SSH key directory or key files are missing".into(),
        ))
    }
}
