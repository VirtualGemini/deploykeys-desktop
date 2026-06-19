use crate::{
    credentials::CredentialStore,
    db::Database,
    models::{AuthMethod, OsType, Target, TargetStatus, TargetType},
    ssh::run_remote_command,
    Error, Result,
};
use chrono::Utc;

/// Service for managing deployment targets
pub struct TargetService {
    db: Database,
}

impl TargetService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Create the default local machine target
    pub async fn create_local_target(&self, key_base_dir: String) -> Result<Target> {
        let os = detect_os();

        let target = Target {
            id: 0,
            target_type: TargetType::Local,
            alias: "Local Machine".to_string(),
            os,
            host: None,
            port: None,
            username: None,
            auth_method: None,
            auth_ref: None,
            key_base_dir,
            status: TargetStatus::Active,
            host_key_fingerprint: None,
            created_at: Utc::now(),
            last_checked_at: Some(Utc::now()),
        };

        let id = self.db.targets().create(&target).await?;

        Ok(Target { id, ..target })
    }

    /// Check if local target exists
    pub async fn local_target_exists(&self) -> Result<bool> {
        Ok(self
            .db
            .targets()
            .find_by_alias("Local Machine")
            .await?
            .is_some())
    }

    pub async fn list_targets(&self) -> Result<Vec<Target>> {
        let mut targets = self.db.targets().list_all().await?;
        if !targets
            .iter()
            .any(|target| target.target_type == TargetType::Local)
        {
            let local = self
                .create_local_target(default_local_key_base_dir()?)
                .await?;
            targets.insert(0, local);
        }
        Ok(targets)
    }

    pub async fn ensure_local_target(&self) -> Result<Target> {
        if let Some(target) = self.db.targets().find_by_alias("Local Machine").await? {
            return Ok(target);
        }
        self.create_local_target(default_local_key_base_dir()?)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_remote_target(
        &self,
        alias: String,
        host: String,
        port: u16,
        username: String,
        auth_method: AuthMethod,
        auth_secret: String,
        key_base_dir: String,
    ) -> Result<Target> {
        let alias = alias.trim().to_string();
        let host = host.trim().to_string();
        let username = username.trim().to_string();
        let auth_secret = auth_secret.trim().to_string();
        let key_base_dir = normalize_key_base_dir(&key_base_dir);

        if alias.is_empty() {
            return Err(Error::Validation("Alias is required".into()));
        }
        if host.is_empty() {
            return Err(Error::Validation("Host is required".into()));
        }
        if username.is_empty() {
            return Err(Error::Validation("Username is required".into()));
        }
        if auth_secret.is_empty() {
            return Err(Error::Validation(match auth_method {
                AuthMethod::Password => "SSH password is required".into(),
                AuthMethod::SshKey => "SSH private key path is required".into(),
            }));
        }
        if self.db.targets().find_by_alias(&alias).await?.is_some() {
            return Err(Error::AlreadyExists(format!(
                "Connection '{}' already exists",
                alias
            )));
        }

        let auth_ref = match auth_method {
            AuthMethod::SshKey => auth_secret,
            AuthMethod::Password => {
                let ref_key = format!(
                    "ssh_password_connection_{}_{}",
                    credential_key_part(&alias),
                    rand::random::<u64>()
                );
                CredentialStore::store_ssh_password_ref(&ref_key, &auth_secret)?
            }
        };

        let mut target = Target {
            id: 0,
            target_type: TargetType::Remote,
            alias,
            os: OsType::Unknown,
            host: Some(host),
            port: Some(port),
            username: Some(username),
            auth_method: Some(auth_method.clone()),
            auth_ref: Some(auth_ref.clone()),
            key_base_dir,
            status: TargetStatus::Unknown,
            host_key_fingerprint: None,
            created_at: Utc::now(),
            last_checked_at: None,
        };

        if let Err(e) = self.check_remote_target(&mut target).await {
            if auth_method == AuthMethod::Password {
                let _ = CredentialStore::delete_credential(&auth_ref);
            }
            return Err(e);
        }
        let id = match self.db.targets().create(&target).await {
            Ok(id) => id,
            Err(e) => {
                if auth_method == AuthMethod::Password {
                    let _ = CredentialStore::delete_credential(&auth_ref);
                }
                return Err(e);
            }
        };
        Ok(Target { id, ..target })
    }

    pub async fn check_target(&self, id: i64) -> Result<Target> {
        let mut target = self
            .db
            .targets()
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::NotFound("Connection not found".into()))?;
        if target.target_type == TargetType::Remote {
            self.check_remote_target(&mut target).await?;
            self.db.targets().update(&target).await?;
        }
        Ok(target)
    }

    pub async fn delete_remote_target(&self, id: i64) -> Result<()> {
        let target = self
            .db
            .targets()
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::NotFound("Connection not found".into()))?;
        if target.target_type == TargetType::Local {
            return Err(Error::Validation(
                "The local connection cannot be deleted".into(),
            ));
        }
        let password_ref = (target.auth_method == Some(AuthMethod::Password))
            .then(|| target.auth_ref.clone())
            .flatten();
        self.db.targets().delete(id).await?;
        if let Some(auth_ref) = password_ref.as_deref() {
            let _ = CredentialStore::delete_credential(auth_ref);
        }
        Ok(())
    }

    async fn check_remote_target(&self, target: &mut Target) -> Result<()> {
        let output =
            match run_remote_command(target, "command -v ssh-keygen >/dev/null; uname -s").await {
                Ok(output) => output,
                Err(e) => {
                    target.status = TargetStatus::Unreachable;
                    target.last_checked_at = Some(Utc::now());
                    return Err(e);
                }
            };

        let os = output.stdout.trim().to_ascii_lowercase();
        target.os = if os.contains("darwin") {
            OsType::MacOs
        } else if os.contains("linux") {
            OsType::Linux
        } else {
            OsType::Unknown
        };
        target.status = TargetStatus::Active;
        target.last_checked_at = Some(Utc::now());
        Ok(())
    }
}

fn detect_os() -> OsType {
    if cfg!(target_os = "macos") {
        OsType::MacOs
    } else if cfg!(target_os = "linux") {
        OsType::Linux
    } else {
        OsType::Unknown
    }
}

fn default_local_key_base_dir() -> Result<String> {
    let home =
        dirs::home_dir().ok_or_else(|| Error::Other("Could not find home directory".into()))?;
    Ok(home
        .join(".ssh")
        .join("deploykeys")
        .to_string_lossy()
        .to_string())
}

fn normalize_key_base_dir(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        "~/.ssh/deploykeys".to_string()
    } else {
        value.trim_end_matches('/').to_string()
    }
}

fn credential_key_part(value: &str) -> String {
    let mut out = String::new();
    let mut last_sep = false;
    for c in value.chars() {
        let next = if c.is_ascii_alphanumeric() {
            Some(c.to_ascii_lowercase())
        } else if matches!(c, '-' | '_' | '.') {
            Some('_')
        } else {
            None
        };
        if let Some(c) = next {
            if c == '_' {
                if !last_sep && !out.is_empty() {
                    out.push(c);
                }
                last_sep = true;
            } else {
                out.push(c);
                last_sep = false;
            }
        }
    }
    let out = out.trim_matches('_');
    if out.is_empty() {
        "remote".to_string()
    } else {
        out.to_string()
    }
}
