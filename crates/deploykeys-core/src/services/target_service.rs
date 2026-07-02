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

        let target = Target {
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

    #[allow(clippy::too_many_arguments)]
    pub async fn test_remote_target_config(
        &self,
        id: Option<i64>,
        host: String,
        port: u16,
        username: String,
        auth_method: AuthMethod,
        auth_secret: String,
    ) -> Result<()> {
        let host = host.trim().to_string();
        let username = username.trim().to_string();
        let auth_secret = auth_secret.trim().to_string();
        if host.is_empty() {
            return Err(Error::Validation("Host is required".into()));
        }
        if username.is_empty() {
            return Err(Error::Validation("Username is required".into()));
        }

        let existing = if let Some(id) = id {
            let target = self
                .db
                .targets()
                .find_by_id(id)
                .await?
                .ok_or_else(|| Error::NotFound("Connection not found".into()))?;
            if target.target_type == TargetType::Local {
                return Err(Error::Validation(
                    "The local connection cannot be tested here".into(),
                ));
            }
            Some(target)
        } else {
            None
        };

        let mut temp_password_ref = None::<String>;
        let auth_ref = match (&existing, &auth_method, auth_secret.is_empty()) {
            (Some(existing), method, true) if existing.auth_method.as_ref() == Some(method) => {
                existing.auth_ref.clone().ok_or_else(|| {
                    Error::Validation("Saved authentication reference is missing".into())
                })?
            }
            (_, AuthMethod::SshKey, false) => auth_secret,
            (_, AuthMethod::Password, false) => {
                let ref_key = format!("ssh_password_connection_test_{}", rand::random::<u64>());
                let stored = CredentialStore::store_ssh_password_ref(&ref_key, &auth_secret)?;
                temp_password_ref = Some(stored.clone());
                stored
            }
            (_, AuthMethod::SshKey, true) => {
                return Err(Error::Validation("SSH private key path is required".into()));
            }
            (_, AuthMethod::Password, true) => {
                return Err(Error::Validation("SSH password is required".into()));
            }
        };

        let mut target = Target {
            id: existing
                .as_ref()
                .map(|target| target.id)
                .unwrap_or_default(),
            target_type: TargetType::Remote,
            alias: existing
                .as_ref()
                .map(|target| target.alias.clone())
                .unwrap_or_else(|| "Connection test".to_string()),
            os: OsType::Unknown,
            host: Some(host),
            port: Some(port),
            username: Some(username),
            auth_method: Some(auth_method),
            auth_ref: Some(auth_ref),
            key_base_dir: existing
                .as_ref()
                .map(|target| target.key_base_dir.clone())
                .unwrap_or_else(|| normalize_key_base_dir("")),
            status: TargetStatus::Unknown,
            host_key_fingerprint: None,
            created_at: existing
                .as_ref()
                .map(|target| target.created_at)
                .unwrap_or_else(Utc::now),
            last_checked_at: None,
        };

        let result = self.check_remote_target(&mut target).await.map(|_| ());
        if let Some(auth_ref) = temp_password_ref.as_deref() {
            let _ = CredentialStore::delete_credential(auth_ref);
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_remote_target(
        &self,
        id: i64,
        alias: String,
        host: String,
        port: u16,
        username: String,
        auth_method: AuthMethod,
        auth_secret: String,
    ) -> Result<Target> {
        let existing = self
            .db
            .targets()
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::NotFound("Connection not found".into()))?;
        if existing.target_type == TargetType::Local {
            return Err(Error::Validation(
                "The local connection cannot be edited".into(),
            ));
        }

        let alias = alias.trim().to_string();
        let host = host.trim().to_string();
        let username = username.trim().to_string();
        let auth_secret = auth_secret.trim().to_string();
        if alias.is_empty() {
            return Err(Error::Validation("Alias is required".into()));
        }
        if host.is_empty() {
            return Err(Error::Validation("Host is required".into()));
        }
        if username.is_empty() {
            return Err(Error::Validation("Username is required".into()));
        }
        if let Some(found) = self.db.targets().find_by_alias(&alias).await? {
            if found.id != id {
                return Err(Error::AlreadyExists(format!(
                    "Connection '{}' already exists",
                    alias
                )));
            }
        }

        let previous_password_ref = (existing.auth_method == Some(AuthMethod::Password))
            .then(|| existing.auth_ref.clone())
            .flatten();
        let mut new_password_ref = None::<String>;
        let auth_ref = match (&existing.auth_method, &auth_method, auth_secret.is_empty()) {
            (Some(AuthMethod::SshKey), AuthMethod::SshKey, true)
            | (Some(AuthMethod::Password), AuthMethod::Password, true) => {
                existing.auth_ref.clone().ok_or_else(|| {
                    Error::Validation("Saved authentication reference is missing".into())
                })?
            }
            (_, AuthMethod::SshKey, false) => auth_secret,
            (_, AuthMethod::Password, false) => {
                let ref_key = format!(
                    "ssh_password_connection_{}_{}",
                    credential_key_part(&alias),
                    rand::random::<u64>()
                );
                let stored = CredentialStore::store_ssh_password_ref(&ref_key, &auth_secret)?;
                new_password_ref = Some(stored.clone());
                stored
            }
            (_, AuthMethod::SshKey, true) => {
                return Err(Error::Validation("SSH private key path is required".into()));
            }
            (_, AuthMethod::Password, true) => {
                return Err(Error::Validation("SSH password is required".into()));
            }
        };

        let next = Target {
            alias,
            os: OsType::Unknown,
            host: Some(host),
            port: Some(port),
            username: Some(username),
            auth_method: Some(auth_method.clone()),
            auth_ref: Some(auth_ref),
            status: TargetStatus::Unknown,
            host_key_fingerprint: None,
            last_checked_at: None,
            ..existing
        };

        if let Err(e) = self.db.targets().update_connection(&next).await {
            if let Some(auth_ref) = new_password_ref.as_deref() {
                let _ = CredentialStore::delete_credential(auth_ref);
            }
            return Err(e);
        }

        if auth_method == AuthMethod::Password {
            if let (Some(previous), Some(new_ref)) = (
                previous_password_ref.as_deref(),
                new_password_ref.as_deref(),
            ) {
                if previous != new_ref {
                    let _ = CredentialStore::delete_credential(previous);
                }
            }
        } else if let Some(previous) = previous_password_ref.as_deref() {
            let _ = CredentialStore::delete_credential(previous);
        }

        Ok(next)
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
        } else if os.contains("mingw") || os.contains("msys") || os.contains("windows") {
            OsType::Windows
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
    } else if cfg!(target_os = "windows") {
        OsType::Windows
    } else {
        OsType::Unknown
    }
}

fn default_local_key_base_dir() -> Result<String> {
    Ok("~/.ssh/deploykeys".to_string())
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
