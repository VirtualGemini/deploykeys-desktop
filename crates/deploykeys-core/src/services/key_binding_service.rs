use crate::{
    credentials::CredentialStore,
    db::Database,
    github::{CreateDeployKeyRequest, GitHubClient},
    keygen::local::LocalKeyGenerator,
    models::{DeployKeyPermission, KeyAlgorithm, KeyBinding, KeyBindingStatus, KeyResidency},
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
        if self
            .db
            .key_bindings()
            .find_by_repo_and_target(repo_id, target_id)
            .await?
            .is_some()
        {
            return Err(Error::AlreadyExists(
                "Key binding already exists for this repository and target".into(),
            ));
        }

        // Resolve credentials first so we fail before touching the filesystem.
        let account = self
            .db
            .accounts()
            .find_by_id(account_id)
            .await?
            .ok_or_else(|| Error::NotFound("Account not found".into()))?;
        let token = get_token_blocking(account.token_ref).await?;

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

        match self.db.key_bindings().create(&binding).await {
            Ok(id) => Ok(KeyBinding { id, ..binding }),
            Err(e) => {
                tracing::warn!(
                    "Binding insert failed; rolling back deploy key {} on {}/{}",
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

    /// Re-check a binding against GitHub and the local filesystem.
    ///
    /// Status transitions follow the drift model from PLAN.md:
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
        let local_key_present = tokio::fs::try_exists(&binding.private_key_path)
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
    async fn duplicate_binding_is_rejected_before_side_effects() {
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

        let server = mockito::Server::new_async().await;
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

        assert!(matches!(error, Error::AlreadyExists(_)));
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
}
