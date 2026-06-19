use crate::{
    credentials::CredentialStore,
    db::Database,
    github::{CreateDeployKeyRequest, GitHubClient},
    keygen::local::LocalKeyGenerator,
    models::{
        DeployKeyPermission, KeyAlgorithm, KeyBinding, KeyBindingStatus, KeyResidency, Repository,
        Target, TargetType,
    },
    ssh::{dirname_remote_path, quote_remote_path, quote_shell, run_remote_command},
    Error, Result,
};
use chrono::Utc;
use std::path::{Path, PathBuf};

/// End-to-end deploy key lifecycle: generate, upload, persist, verify.
pub struct KeyBindingService {
    db: Database,
    github: GitHubClient,
}

impl KeyBindingService {
    pub fn new(db: Database) -> Result<Self> {
        Ok(Self {
            db,
            github: GitHubClient::new()?,
        })
    }

    /// Inject a preconfigured client (tests, GitHub Enterprise).
    pub fn with_github_client(db: Database, github: GitHubClient) -> Self {
        Self { db, github }
    }

    pub async fn ensure_ssh_config_for_binding(
        &self,
        target: &Target,
        repo: &Repository,
        binding: &KeyBinding,
    ) -> Result<()> {
        ensure_repo_ssh_config(target, repo, binding).await
    }

    /// Generate a key pair, upload the public key as a GitHub deploy key, and
    /// persist the binding.
    ///
    /// The three side effects cannot share a transaction, so failures roll
    /// back best-effort: an upload failure removes the local key files, and a
    /// database failure deletes the just-uploaded deploy key as well.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_and_upload_key(
        &self,
        account_id: i64,
        repo_id: i64,
        target_id: i64,
        owner: &str,
        repo_name: &str,
        algorithm: KeyAlgorithm,
        permission: DeployKeyPermission,
        key_path: PathBuf,
        title: String,
    ) -> Result<KeyBinding> {
        // Resolve credentials first so we fail before touching the filesystem.
        let account = self
            .db
            .accounts()
            .find_by_id(account_id)
            .await?
            .ok_or_else(|| Error::NotFound("Account not found".into()))?;
        let token = get_token_blocking(account.token_ref).await?;
        let stale_binding = self
            .stale_binding_slot(&token, owner, repo_name, repo_id, target_id)
            .await?;

        if let Some(parent) = key_path.parent() {
            ensure_private_dir(parent).await?;
        }

        // Key generation (RSA in particular) is CPU-bound; keep it off the
        // async runtime threads.
        let comment = format!("deploykeys:repo_{}:target_{}", repo_id, target_id);
        let gen_algorithm = algorithm.clone();
        let gen_path = key_path.clone();
        let key_pair = tokio::task::spawn_blocking(move || {
            LocalKeyGenerator::generate(gen_algorithm, &gen_path, &comment)
        })
        .await
        .map_err(|e| Error::Other(format!("Key generation task failed: {}", e)))??;

        let request = CreateDeployKeyRequest {
            title: title.clone(),
            key: key_pair.public_key.clone(),
            read_only: matches!(permission, DeployKeyPermission::ReadOnly),
        };

        let deploy_key = match self
            .github
            .create_deploy_key(&token, owner, repo_name, &request)
            .await
        {
            Ok(key) => key,
            Err(e) => {
                remove_local_key_files(&key_path).await;
                return Err(e);
            }
        };

        let binding = KeyBinding {
            id: 0,
            repo_id,
            target_id,
            github_deploy_key_id: Some(deploy_key.id),
            deploy_key_title: title,
            algorithm,
            permission,
            public_key: key_pair.public_key,
            public_key_fingerprint: key_pair.fingerprint,
            private_key_path: key_path.to_string_lossy().to_string(),
            private_key_residency: KeyResidency::Local,
            status: KeyBindingStatus::Active,
            created_at: Utc::now(),
            last_verified_at: Some(Utc::now()),
        };

        match self
            .save_uploaded_binding(binding, stale_binding.as_ref())
            .await
        {
            Ok(binding) => Ok(binding),
            Err(e) => {
                tracing::warn!(
                    "Binding save failed; rolling back deploy key {} on {}/{}",
                    deploy_key.id,
                    owner,
                    repo_name
                );
                if let Err(rollback) = self
                    .github
                    .delete_deploy_key(&token, owner, repo_name, deploy_key.id)
                    .await
                {
                    tracing::error!(
                        "Rollback failed; deploy key {} on {}/{} is orphaned: {}",
                        deploy_key.id,
                        owner,
                        repo_name,
                        rollback
                    );
                }
                remove_local_key_files(&key_path).await;
                Err(e)
            }
        }
    }

    /// Upload an existing local SSH public key as a GitHub deploy key, then
    /// persist the binding between the repository and the key's target.
    pub async fn upload_existing_key(
        &self,
        repo_id: i64,
        ssh_key_id: i64,
        token: &str,
        permission: DeployKeyPermission,
    ) -> Result<KeyBinding> {
        let repo = self
            .db
            .repositories()
            .find_by_id(repo_id)
            .await?
            .ok_or_else(|| Error::NotFound("Repository not found".into()))?;

        let ssh_key = self
            .db
            .ssh_keys()
            .find_by_id(ssh_key_id)
            .await?
            .ok_or_else(|| Error::NotFound("SSH key not found".into()))?;
        let target = self
            .db
            .targets()
            .find_by_id(ssh_key.target_id)
            .await?
            .ok_or_else(|| Error::NotFound("Target not found".into()))?;
        let stale_binding = self
            .stale_binding_slot(token, &repo.owner, &repo.name, repo.id, ssh_key.target_id)
            .await?;

        let public_key = read_target_public_key(&target, &ssh_key).await?;
        if public_key.is_empty() {
            return Err(Error::Validation("SSH public key is empty".into()));
        }

        let title = format!("DeployKeys - {}", ssh_key.directory);
        let request = CreateDeployKeyRequest {
            title: title.clone(),
            key: public_key.clone(),
            read_only: matches!(permission, DeployKeyPermission::ReadOnly),
        };

        let deploy_key = self
            .github
            .create_deploy_key(token, &repo.owner, &repo.name, &request)
            .await?;

        let binding = KeyBinding {
            id: 0,
            repo_id: repo.id,
            target_id: ssh_key.target_id,
            github_deploy_key_id: Some(deploy_key.id),
            deploy_key_title: title,
            algorithm: ssh_key.algorithm,
            permission,
            public_key,
            public_key_fingerprint: ssh_key.public_key_fingerprint,
            private_key_path: ssh_key.private_key_path,
            private_key_residency: if target.target_type == TargetType::Remote {
                KeyResidency::Remote
            } else {
                KeyResidency::Local
            },
            status: KeyBindingStatus::Active,
            created_at: Utc::now(),
            last_verified_at: Some(Utc::now()),
        };

        match self
            .save_uploaded_binding(binding, stale_binding.as_ref())
            .await
        {
            Ok(binding) => {
                if let Err(e) = ensure_repo_ssh_config(&target, &repo, &binding).await {
                    tracing::warn!(
                        "SSH config update failed; rolling back deploy key {} on {}/{}",
                        deploy_key.id,
                        repo.owner,
                        repo.name
                    );
                    if let Err(db_rollback) = self.db.key_bindings().delete(binding.id).await {
                        tracing::error!(
                            "Rollback failed; binding {} could not be deleted: {}",
                            binding.id,
                            db_rollback
                        );
                    }
                    if let Err(remote_rollback) = self
                        .github
                        .delete_deploy_key(token, &repo.owner, &repo.name, deploy_key.id)
                        .await
                    {
                        tracing::error!(
                            "Rollback failed; deploy key {} on {}/{} is orphaned: {}",
                            deploy_key.id,
                            repo.owner,
                            repo.name,
                            remote_rollback
                        );
                    }
                    return Err(e);
                }
                Ok(binding)
            }
            Err(e) => {
                tracing::warn!(
                    "Binding save failed; rolling back deploy key {} on {}/{}",
                    deploy_key.id,
                    repo.owner,
                    repo.name
                );
                if let Err(rollback) = self
                    .github
                    .delete_deploy_key(token, &repo.owner, &repo.name, deploy_key.id)
                    .await
                {
                    tracing::error!(
                        "Rollback failed; deploy key {} on {}/{} is orphaned: {}",
                        deploy_key.id,
                        repo.owner,
                        repo.name,
                        rollback
                    );
                }
                Err(e)
            }
        }
    }

    /// Re-check a binding against GitHub and the local filesystem.
    ///
    /// Status transitions follow the drift model:
    /// - deploy key missing on GitHub  -> `Drifted`
    /// - local private key missing     -> `OrphanedRemote`
    /// - both present                  -> `Active`
    ///
    /// Returns `true` only when the binding is fully healthy.
    pub async fn verify_key(&self, binding_id: i64) -> Result<bool> {
        let binding = self
            .db
            .key_bindings()
            .find_by_id(binding_id)
            .await?
            .ok_or_else(|| Error::NotFound("Key binding not found".into()))?;

        let repo = self
            .db
            .repositories()
            .find_by_id(binding.repo_id)
            .await?
            .ok_or_else(|| Error::NotFound("Repository not found".into()))?;

        let account = self
            .db
            .accounts()
            .find_by_id(repo.account_id)
            .await?
            .ok_or_else(|| Error::NotFound("Account not found".into()))?;

        let token = get_token_blocking(account.token_ref).await?;

        let keys = self
            .github
            .list_deploy_keys(&token, &repo.owner, &repo.name)
            .await?;

        let github_key_present = keys
            .iter()
            .any(|k| Some(k.id) == binding.github_deploy_key_id);
        let target = self
            .db
            .targets()
            .find_by_id(binding.target_id)
            .await?
            .ok_or_else(|| Error::NotFound("Target not found".into()))?;
        let local_key_present = target_private_key_exists(&target, &binding.private_key_path)
            .await
            .unwrap_or(false);

        let status = match (github_key_present, local_key_present) {
            (true, true) => KeyBindingStatus::Active,
            (false, _) => KeyBindingStatus::Drifted,
            (true, false) => KeyBindingStatus::OrphanedRemote,
        };
        let healthy = status == KeyBindingStatus::Active;

        self.db
            .key_bindings()
            .update_status(binding_id, status)
            .await?;

        Ok(healthy)
    }

    async fn stale_binding_slot(
        &self,
        token: &str,
        owner: &str,
        repo_name: &str,
        repo_id: i64,
        target_id: i64,
    ) -> Result<Option<KeyBinding>> {
        let Some(binding) = self
            .db
            .key_bindings()
            .find_by_repo_and_target(repo_id, target_id)
            .await?
        else {
            return Ok(None);
        };

        if self
            .remote_binding_exists(token, owner, repo_name, &binding)
            .await?
        {
            return Err(Error::AlreadyExists(
                "Key binding already exists for this repository and target".into(),
            ));
        }

        tracing::info!(
            "Replacing stale key binding {} for repository {} and target {}",
            binding.id,
            repo_id,
            target_id
        );

        Ok(Some(binding))
    }

    async fn remote_binding_exists(
        &self,
        token: &str,
        owner: &str,
        repo_name: &str,
        binding: &KeyBinding,
    ) -> Result<bool> {
        let Some(deploy_key_id) = binding.github_deploy_key_id else {
            return Ok(false);
        };

        let keys = self
            .github
            .list_deploy_keys(token, owner, repo_name)
            .await?;
        Ok(keys.iter().any(|key| key.id == deploy_key_id))
    }

    async fn save_uploaded_binding(
        &self,
        binding: KeyBinding,
        stale_binding: Option<&KeyBinding>,
    ) -> Result<KeyBinding> {
        if let Some(stale_binding) = stale_binding {
            let binding = KeyBinding {
                id: stale_binding.id,
                ..binding
            };
            self.db.key_bindings().replace(&binding).await?;
            return Ok(binding);
        }

        let id = self.db.key_bindings().create(&binding).await?;
        Ok(KeyBinding { id, ..binding })
    }
}

async fn get_token_blocking(token_ref: String) -> Result<String> {
    tokio::task::spawn_blocking(move || CredentialStore::get_token(&token_ref))
        .await
        .map_err(|e| Error::Other(format!("Keyring task failed: {}", e)))?
}

async fn ensure_private_dir(parent: &Path) -> Result<()> {
    let existed = parent.exists();
    tokio::fs::create_dir_all(parent).await?;

    #[cfg(unix)]
    if !existed {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).await?;
    }
    #[cfg(not(unix))]
    let _ = existed;

    Ok(())
}

async fn remove_local_key_files(private_path: &Path) {
    let _ = tokio::fs::remove_file(private_path).await;
    let _ = tokio::fs::remove_file(private_path.with_extension("pub")).await;
}

async fn ensure_repo_ssh_config(
    target: &Target,
    repo: &crate::models::Repository,
    binding: &KeyBinding,
) -> Result<()> {
    if target.target_type == TargetType::Remote {
        return ensure_remote_repo_ssh_config(target, repo, binding).await;
    }

    let home =
        dirs::home_dir().ok_or_else(|| Error::Other("Could not find home directory".into()))?;
    let ssh_dir = home.join(".ssh");
    tokio::fs::create_dir_all(&ssh_dir).await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&ssh_dir, std::fs::Permissions::from_mode(0o700)).await?;
    }

    let config_path = ssh_dir.join("config");
    let current = match tokio::fs::read_to_string(&config_path).await {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let marker_id = format!("repo:{}", repo.id);
    let begin = format!("# >>> deploykeys-desktop {marker_id}");
    let end = format!("# <<< deploykeys-desktop {marker_id}");
    let host_alias = repo_ssh_host_alias(repo);
    let block = format!(
        "{begin}\n\
         # Repository: {full_name}\n\
         Host {host_alias}\n\
             HostName github.com\n\
             User git\n\
            IdentityFile {identity_file}\n\
             IdentitiesOnly yes\n\
             HostKeyAlias github.com\n\
             StrictHostKeyChecking accept-new\n\
         {end}\n",
        full_name = repo.full_name,
        identity_file = quote_ssh_config_value(&binding.private_key_path),
    );

    let next = replace_managed_block(&current, &begin, &end, &block);
    tokio::fs::write(&config_path, next).await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o600)).await?;
    }

    Ok(())
}

async fn ensure_remote_repo_ssh_config(
    target: &Target,
    repo: &crate::models::Repository,
    binding: &KeyBinding,
) -> Result<()> {
    let marker_id = format!("repo:{}", repo.id);
    let begin = format!("# >>> deploykeys-desktop {marker_id}");
    let end = format!("# <<< deploykeys-desktop {marker_id}");
    let host_alias = repo_ssh_host_alias(repo);
    let block = format!(
        "{begin}\n\
         # Repository: {full_name}\n\
         Host {host_alias}\n\
             HostName github.com\n\
             User git\n\
             IdentityFile {identity_file}\n\
             IdentitiesOnly yes\n\
             HostKeyAlias github.com\n\
             StrictHostKeyChecking accept-new\n\
         {end}\n",
        full_name = repo.full_name,
        identity_file = quote_ssh_config_value(&binding.private_key_path),
    );
    let sed_range = format!("/^{begin}$/,/^{end}$/d");
    let script = format!(
        "set -eu; \
         mkdir -p ~/.ssh; chmod 700 ~/.ssh; touch ~/.ssh/config; chmod 600 ~/.ssh/config; \
         tmp=$(mktemp); \
         sed {sed_range} ~/.ssh/config > \"$tmp\"; \
         if [ -s \"$tmp\" ]; then printf '\\n' >> \"$tmp\"; fi; \
         printf '%s\\n' {block} >> \"$tmp\"; \
         mv \"$tmp\" ~/.ssh/config; chmod 600 ~/.ssh/config",
        sed_range = quote_shell(&sed_range),
        block = quote_shell(block.trim_end_matches('\n')),
    );
    run_remote_command(target, &script).await?;
    Ok(())
}

async fn read_target_public_key(
    target: &Target,
    ssh_key: &crate::models::SshKey,
) -> Result<String> {
    match target.target_type {
        TargetType::Local => {
            if !tokio::fs::try_exists(&ssh_key.private_key_path)
                .await
                .unwrap_or(false)
                || !tokio::fs::try_exists(&ssh_key.public_key_path)
                    .await
                    .unwrap_or(false)
            {
                return Err(Error::NotFound(
                    "SSH key directory or key files are missing".into(),
                ));
            }
            Ok(tokio::fs::read_to_string(&ssh_key.public_key_path)
                .await?
                .trim()
                .to_string())
        }
        TargetType::Remote => {
            let dir = dirname_remote_path(&ssh_key.private_key_path).unwrap_or_default();
            let command = format!(
                "test -d {dir} && test -f {private_key} && test -f {public_key} && cat {public_key}",
                dir = quote_remote_path(&dir),
                private_key = quote_remote_path(&ssh_key.private_key_path),
                public_key = quote_remote_path(&ssh_key.public_key_path),
            );
            Ok(run_remote_command(target, &command)
                .await?
                .stdout
                .trim()
                .to_string())
        }
    }
}

async fn target_private_key_exists(target: &Target, private_key_path: &str) -> Result<bool> {
    match target.target_type {
        TargetType::Local => Ok(tokio::fs::try_exists(private_key_path)
            .await
            .unwrap_or(false)),
        TargetType::Remote => {
            let command = format!("test -f {}", quote_remote_path(private_key_path));
            Ok(run_remote_command(target, &command).await.is_ok())
        }
    }
}

fn repo_ssh_host_alias(repo: &crate::models::Repository) -> String {
    let readable = sanitize_host_part(&repo.full_name.replace('/', "-"));
    format!("deploykeys-{}-{}", repo.id, readable)
}

fn sanitize_host_part(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for c in value.chars() {
        let next = if c.is_ascii_alphanumeric() {
            Some(c.to_ascii_lowercase())
        } else if matches!(c, '-' | '_' | '.') {
            Some('-')
        } else {
            None
        };

        if let Some(c) = next {
            if c == '-' {
                if !last_dash && !out.is_empty() {
                    out.push(c);
                }
                last_dash = true;
            } else {
                out.push(c);
                last_dash = false;
            }
        }
    }
    out.trim_matches('-').to_string()
}

fn quote_ssh_config_value(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn replace_managed_block(current: &str, begin: &str, end: &str, block: &str) -> String {
    if let Some(start) = current.find(begin) {
        if let Some(relative_end) = current[start..].find(end) {
            let end_index = start + relative_end + end.len();
            let after_end = current[end_index..]
                .strip_prefix('\n')
                .unwrap_or(&current[end_index..]);
            let mut next = String::new();
            next.push_str(current[..start].trim_end_matches('\n'));
            if !next.is_empty() {
                next.push_str("\n\n");
            }
            next.push_str(block.trim_end_matches('\n'));
            if !after_end.trim().is_empty() {
                next.push_str("\n\n");
                next.push_str(after_end.trim_start_matches('\n'));
            } else {
                next.push('\n');
            }
            return next;
        }
    }

    let mut next = current.trim_end_matches('\n').to_string();
    if !next.is_empty() {
        next.push_str("\n\n");
    }
    next.push_str(block);
    next
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::test_support::use_mock_keyring;
    use crate::db::test_support::{seed_account, seed_repository, seed_target, test_db};

    async fn seeded(db: &Database) -> (i64, i64, i64) {
        let account_id = seed_account(db).await;
        let repo_id = seed_repository(db, account_id).await;
        let target_id = seed_target(db).await;
        (account_id, repo_id, target_id)
    }

    #[tokio::test]
    async fn create_and_upload_key_happy_path_persists_binding() {
        use_mock_keyring();
        let (_dir, db) = test_db().await;
        let (account_id, repo_id, target_id) = seeded(&db).await;
        let account = db.accounts().find_by_id(account_id).await.unwrap().unwrap();
        CredentialStore::store_token("seeded", "ghu_seeded_token").unwrap();
        assert_eq!(account.token_ref, "github_token_seeded");

        let mut server = mockito::Server::new_async().await;
        let upload = server
            .mock("POST", "/repos/owner/repo/keys")
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"id": 77, "key": "k", "url": "u", "title": "t",
                    "verified": true, "created_at": "now", "read_only": true}"#,
            )
            .create_async()
            .await;

        let github = GitHubClient::new().unwrap().with_base_url(server.url());
        let service = KeyBindingService::with_github_client(db.clone(), github);

        let key_dir = tempfile::TempDir::new().unwrap();
        let key_path = key_dir.path().join("keys").join("id_ed25519");

        let binding = service
            .create_and_upload_key(
                account_id,
                repo_id,
                target_id,
                "owner",
                "repo",
                KeyAlgorithm::Ed25519,
                DeployKeyPermission::ReadOnly,
                key_path.clone(),
                "deploykeys test".to_string(),
            )
            .await
            .unwrap();

        upload.assert_async().await;
        assert!(binding.id > 0);
        assert_eq!(binding.github_deploy_key_id, Some(77));
        assert_eq!(binding.status, KeyBindingStatus::Active);
        assert!(key_path.exists());
        assert!(key_path.with_extension("pub").exists());

        let stored = db
            .key_bindings()
            .find_by_repo_and_target(repo_id, target_id)
            .await
            .unwrap();
        assert!(stored.is_some());
    }

    #[tokio::test]
    async fn upload_failure_cleans_up_local_key_files() {
        use_mock_keyring();
        let (_dir, db) = test_db().await;
        let (account_id, repo_id, target_id) = seeded(&db).await;
        CredentialStore::store_token("seeded", "ghu_seeded_token").unwrap();

        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/repos/owner/repo/keys")
            .with_status(422)
            .with_header("content-type", "application/json")
            .with_body(r#"{"message": "key is already in use"}"#)
            .create_async()
            .await;

        let github = GitHubClient::new().unwrap().with_base_url(server.url());
        let service = KeyBindingService::with_github_client(db.clone(), github);

        let key_dir = tempfile::TempDir::new().unwrap();
        let key_path = key_dir.path().join("id_ed25519");

        let error = service
            .create_and_upload_key(
                account_id,
                repo_id,
                target_id,
                "owner",
                "repo",
                KeyAlgorithm::Ed25519,
                DeployKeyPermission::ReadOnly,
                key_path.clone(),
                "deploykeys test".to_string(),
            )
            .await
            .unwrap_err();

        assert!(matches!(error, Error::GitHub(_)));
        assert!(!key_path.exists(), "private key must be cleaned up");
        assert!(!key_path.with_extension("pub").exists());

        let stored = db
            .key_bindings()
            .find_by_repo_and_target(repo_id, target_id)
            .await
            .unwrap();
        assert!(stored.is_none());
    }

    #[tokio::test]
    async fn duplicate_binding_is_rejected_when_remote_key_still_exists() {
        use_mock_keyring();
        let (_dir, db) = test_db().await;
        let (account_id, repo_id, target_id) = seeded(&db).await;
        CredentialStore::store_token("seeded", "ghu_seeded_token").unwrap();

        let binding = KeyBinding {
            id: 0,
            repo_id,
            target_id,
            github_deploy_key_id: Some(1),
            deploy_key_title: "existing".to_string(),
            algorithm: KeyAlgorithm::Ed25519,
            permission: DeployKeyPermission::ReadOnly,
            public_key: "k".to_string(),
            public_key_fingerprint: "f".to_string(),
            private_key_path: "/tmp/none".to_string(),
            private_key_residency: KeyResidency::Local,
            status: KeyBindingStatus::Active,
            created_at: Utc::now(),
            last_verified_at: None,
        };
        db.key_bindings().create(&binding).await.unwrap();

        let mut server = mockito::Server::new_async().await;
        let remote_key = server
            .mock("GET", "/repos/owner/repo/keys")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[{"id": 1, "key": "k", "url": "u", "title": "existing",
                    "verified": true, "created_at": "now", "read_only": true}]"#,
            )
            .create_async()
            .await;
        let github = GitHubClient::new().unwrap().with_base_url(server.url());
        let service = KeyBindingService::with_github_client(db.clone(), github);

        let error = service
            .create_and_upload_key(
                account_id,
                repo_id,
                target_id,
                "owner",
                "repo",
                KeyAlgorithm::Ed25519,
                DeployKeyPermission::ReadOnly,
                PathBuf::from("/tmp/never_created"),
                "dup".to_string(),
            )
            .await
            .unwrap_err();

        remote_key.assert_async().await;
        assert!(matches!(error, Error::AlreadyExists(_)));
    }

    #[tokio::test]
    async fn stale_local_binding_is_replaced_when_remote_key_is_missing() {
        use_mock_keyring();
        let (_dir, db) = test_db().await;
        let (account_id, repo_id, target_id) = seeded(&db).await;
        CredentialStore::store_token("seeded", "ghu_seeded_token").unwrap();

        let stale = KeyBinding {
            id: 0,
            repo_id,
            target_id,
            github_deploy_key_id: Some(1),
            deploy_key_title: "stale".to_string(),
            algorithm: KeyAlgorithm::Ed25519,
            permission: DeployKeyPermission::ReadOnly,
            public_key: "old-public-key".to_string(),
            public_key_fingerprint: "old-fingerprint".to_string(),
            private_key_path: "/tmp/stale".to_string(),
            private_key_residency: KeyResidency::Local,
            status: KeyBindingStatus::Active,
            created_at: Utc::now(),
            last_verified_at: None,
        };
        let stale_id = db.key_bindings().create(&stale).await.unwrap();

        let mut server = mockito::Server::new_async().await;
        let list = server
            .mock("GET", "/repos/owner/repo/keys")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("[]")
            .create_async()
            .await;
        let upload = server
            .mock("POST", "/repos/owner/repo/keys")
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"id": 77, "key": "k", "url": "u", "title": "replacement",
                    "verified": true, "created_at": "now", "read_only": true}"#,
            )
            .create_async()
            .await;

        let github = GitHubClient::new().unwrap().with_base_url(server.url());
        let service = KeyBindingService::with_github_client(db.clone(), github);

        let key_dir = tempfile::TempDir::new().unwrap();
        let key_path = key_dir.path().join("id_ed25519");
        let binding = service
            .create_and_upload_key(
                account_id,
                repo_id,
                target_id,
                "owner",
                "repo",
                KeyAlgorithm::Ed25519,
                DeployKeyPermission::ReadOnly,
                key_path.clone(),
                "replacement".to_string(),
            )
            .await
            .unwrap();

        list.assert_async().await;
        upload.assert_async().await;
        assert_eq!(binding.id, stale_id);
        assert_eq!(binding.github_deploy_key_id, Some(77));
        assert_eq!(binding.status, KeyBindingStatus::Active);
        assert_eq!(binding.private_key_path, key_path.to_string_lossy());
        assert_ne!(binding.public_key, "old-public-key");

        let stored = db
            .key_bindings()
            .find_by_repo_and_target(repo_id, target_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.id, stale_id);
        assert_eq!(stored.github_deploy_key_id, Some(77));
        assert_eq!(stored.deploy_key_title, "replacement");
    }

    #[tokio::test]
    async fn verify_key_marks_drifted_when_github_key_missing() {
        use_mock_keyring();
        let (_dir, db) = test_db().await;
        let (_account_id, repo_id, target_id) = seeded(&db).await;
        CredentialStore::store_token("seeded", "ghu_seeded_token").unwrap();

        // A local key file that exists.
        let key_dir = tempfile::TempDir::new().unwrap();
        let key_path = key_dir.path().join("id_ed25519");
        std::fs::write(&key_path, "private").unwrap();

        let binding = KeyBinding {
            id: 0,
            repo_id,
            target_id,
            github_deploy_key_id: Some(123),
            deploy_key_title: "t".to_string(),
            algorithm: KeyAlgorithm::Ed25519,
            permission: DeployKeyPermission::ReadOnly,
            public_key: "k".to_string(),
            public_key_fingerprint: "f".to_string(),
            private_key_path: key_path.to_string_lossy().to_string(),
            private_key_residency: KeyResidency::Local,
            status: KeyBindingStatus::Active,
            created_at: Utc::now(),
            last_verified_at: None,
        };
        let binding_id = db.key_bindings().create(&binding).await.unwrap();

        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/repos/owner/repo/keys")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("[]")
            .create_async()
            .await;

        let github = GitHubClient::new().unwrap().with_base_url(server.url());
        let service = KeyBindingService::with_github_client(db.clone(), github);

        let healthy = service.verify_key(binding_id).await.unwrap();
        assert!(!healthy);

        let updated = db
            .key_bindings()
            .find_by_id(binding_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.status, KeyBindingStatus::Drifted);
    }

    #[tokio::test]
    async fn verify_key_marks_orphaned_remote_when_local_key_missing() {
        use_mock_keyring();
        let (_dir, db) = test_db().await;
        let (_account_id, repo_id, target_id) = seeded(&db).await;
        CredentialStore::store_token("seeded", "ghu_seeded_token").unwrap();

        let binding = KeyBinding {
            id: 0,
            repo_id,
            target_id,
            github_deploy_key_id: Some(123),
            deploy_key_title: "t".to_string(),
            algorithm: KeyAlgorithm::Ed25519,
            permission: DeployKeyPermission::ReadOnly,
            public_key: "k".to_string(),
            public_key_fingerprint: "f".to_string(),
            private_key_path: "/nonexistent/id_ed25519".to_string(),
            private_key_residency: KeyResidency::Local,
            status: KeyBindingStatus::Active,
            created_at: Utc::now(),
            last_verified_at: None,
        };
        let binding_id = db.key_bindings().create(&binding).await.unwrap();

        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/repos/owner/repo/keys")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[{"id": 123, "key": "k", "url": "u", "title": "t",
                     "verified": true, "created_at": "now", "read_only": true}]"#,
            )
            .create_async()
            .await;

        let github = GitHubClient::new().unwrap().with_base_url(server.url());
        let service = KeyBindingService::with_github_client(db.clone(), github);

        let healthy = service.verify_key(binding_id).await.unwrap();
        assert!(!healthy);

        let updated = db
            .key_bindings()
            .find_by_id(binding_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.status, KeyBindingStatus::OrphanedRemote);
    }

    #[test]
    fn replace_managed_block_appends_when_missing() {
        let current = "Host github.com\n    User git\n";
        let block = "# >>> deploykeys-desktop repo:1\nHost deploykeys-1-owner-repo\n# <<< deploykeys-desktop repo:1\n";

        let next = replace_managed_block(
            current,
            "# >>> deploykeys-desktop repo:1",
            "# <<< deploykeys-desktop repo:1",
            block,
        );

        assert!(next.starts_with(current.trim_end()));
        assert!(next.contains("Host deploykeys-1-owner-repo"));
    }

    #[test]
    fn replace_managed_block_replaces_existing_block() {
        let current = "before\n\n# >>> deploykeys-desktop repo:1\nold\n# <<< deploykeys-desktop repo:1\n\nafter\n";
        let block = "# >>> deploykeys-desktop repo:1\nnew\n# <<< deploykeys-desktop repo:1\n";

        let next = replace_managed_block(
            current,
            "# >>> deploykeys-desktop repo:1",
            "# <<< deploykeys-desktop repo:1",
            block,
        );

        assert!(next.contains("before"));
        assert!(next.contains("new"));
        assert!(next.contains("after"));
        assert!(!next.contains("old"));
    }

    #[test]
    fn ssh_config_value_is_quoted_and_escaped() {
        assert_eq!(
            quote_ssh_config_value(r#"/Users/me/SSH Keys/"prod"/id_ed25519"#),
            r#""/Users/me/SSH Keys/\"prod\"/id_ed25519""#
        );
    }
}
