use crate::db::test_support::{seed_account, seed_repository, seed_target, test_db};
use crate::db::Database;
use crate::models::{
    DeployKeyPermission, KeyAlgorithm, KeyBinding, KeyBindingStatus, KeyResidency,
};
use chrono::Utc;
use tempfile::TempDir;

#[tokio::test]
async fn database_is_created_and_migrated_on_fresh_path() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("nested").join("test.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();

    let db = Database::new(&db_path).await.unwrap();
    db.run_migrations().await.unwrap();

    assert!(db_path.exists());

    // Migrations are idempotent.
    db.run_migrations().await.unwrap();
}

#[tokio::test]
async fn empty_database_path_is_rejected() {
    let result = Database::new(std::path::Path::new("")).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn account_crud_roundtrip() {
    use crate::models::{Account, AuthType};

    let (_dir, db) = test_db().await;

    let account = Account {
        id: 0,
        github_user_id: 12345,
        login: "testuser".to_string(),
        avatar_url: Some("https://example.com/avatar.png".to_string()),
        auth_type: AuthType::GitHubAppDeviceFlow,
        token_ref: "test_token_ref".to_string(),
        refresh_token_ref: Some("test_refresh_ref".to_string()),
        token_expires_at: Some(Utc::now()),
        created_at: Utc::now(),
        last_login_at: Utc::now(),
    };

    let id = db.accounts().create(&account).await.unwrap();
    assert!(id > 0);

    let retrieved = db.accounts().find_by_id(id).await.unwrap().unwrap();
    assert_eq!(retrieved.login, "testuser");
    assert_eq!(retrieved.auth_type, AuthType::GitHubAppDeviceFlow);
    assert_eq!(
        retrieved.refresh_token_ref.as_deref(),
        Some("test_refresh_ref")
    );

    let by_github_id = db
        .accounts()
        .find_by_github_user_id(12345)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(by_github_id.id, id);

    let mut updated = retrieved;
    updated.login = "renamed".to_string();
    db.accounts().update(&updated).await.unwrap();
    let reloaded = db.accounts().find_by_id(id).await.unwrap().unwrap();
    assert_eq!(reloaded.login, "renamed");

    db.accounts().delete(id).await.unwrap();
    assert!(db.accounts().find_by_id(id).await.unwrap().is_none());
}

#[tokio::test]
async fn target_crud_roundtrip() {
    use crate::models::{OsType, Target, TargetStatus, TargetType};

    let (_dir, db) = test_db().await;

    let target = Target {
        id: 0,
        target_type: TargetType::Local,
        alias: "Test Machine".to_string(),
        os: OsType::Linux,
        host: None,
        port: Some(2222),
        username: None,
        auth_method: None,
        auth_ref: None,
        key_base_dir: "/tmp/keys".to_string(),
        status: TargetStatus::Active,
        host_key_fingerprint: None,
        created_at: Utc::now(),
        last_checked_at: None,
    };

    let id = db.targets().create(&target).await.unwrap();
    assert!(id > 0);

    let retrieved = db.targets().find_by_id(id).await.unwrap().unwrap();
    assert_eq!(retrieved.alias, "Test Machine");
    assert_eq!(retrieved.port, Some(2222));

    let by_alias = db.targets().find_by_alias("Test Machine").await.unwrap();
    assert!(by_alias.is_some());
}

fn sample_binding(repo_id: i64, target_id: i64) -> KeyBinding {
    KeyBinding {
        id: 0,
        repo_id,
        target_id,
        github_deploy_key_id: None,
        deploy_key_title: "test_key".to_string(),
        algorithm: KeyAlgorithm::Ed25519,
        permission: DeployKeyPermission::ReadOnly,
        public_key: "ssh-ed25519 AAAA...".to_string(),
        public_key_fingerprint: "SHA256:abc123".to_string(),
        private_key_path: "/tmp/id_ed25519".to_string(),
        private_key_residency: KeyResidency::Local,
        status: KeyBindingStatus::Active,
        created_at: Utc::now(),
        last_verified_at: None,
    }
}

#[tokio::test]
async fn key_binding_unique_constraint_is_enforced() {
    let (_dir, db) = test_db().await;
    let account_id = seed_account(&db).await;
    let repo_id = seed_repository(&db, account_id).await;
    let target_id = seed_target(&db).await;

    let binding = sample_binding(repo_id, target_id);

    let id1 = db.key_bindings().create(&binding).await.unwrap();
    assert!(id1 > 0);

    // Same (repo_id, target_id) pair must be rejected.
    let result = db.key_bindings().create(&binding).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn key_binding_requires_existing_repo_and_target() {
    let (_dir, db) = test_db().await;

    // No rows seeded: foreign keys must reject the insert.
    let result = db.key_bindings().create(&sample_binding(999, 999)).await;
    assert!(result.is_err(), "foreign_keys pragma must be enforced");
}

#[tokio::test]
async fn deleting_repository_cascades_to_bindings() {
    let (_dir, db) = test_db().await;
    let account_id = seed_account(&db).await;
    let repo_id = seed_repository(&db, account_id).await;
    let target_id = seed_target(&db).await;

    let binding_id = db
        .key_bindings()
        .create(&sample_binding(repo_id, target_id))
        .await
        .unwrap();

    db.repositories().delete(repo_id).await.unwrap();

    let orphan = db.key_bindings().find_by_id(binding_id).await.unwrap();
    assert!(orphan.is_none(), "ON DELETE CASCADE must remove bindings");
}

#[tokio::test]
async fn update_status_stamps_verification_time() {
    let (_dir, db) = test_db().await;
    let account_id = seed_account(&db).await;
    let repo_id = seed_repository(&db, account_id).await;
    let target_id = seed_target(&db).await;

    let binding_id = db
        .key_bindings()
        .create(&sample_binding(repo_id, target_id))
        .await
        .unwrap();

    db.key_bindings()
        .update_status(binding_id, KeyBindingStatus::Drifted)
        .await
        .unwrap();

    let updated = db
        .key_bindings()
        .find_by_id(binding_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.status, KeyBindingStatus::Drifted);
    assert!(updated.last_verified_at.is_some());
}

#[tokio::test]
async fn repository_update_changes_fields_in_place() {
    use crate::models::Repository;

    let (_dir, db) = test_db().await;
    let account_id = seed_account(&db).await;
    seed_repository(&db, account_id).await; // github_repo_id = 1337

    let existing = db
        .repositories()
        .find_by_github_repo_id(1337)
        .await
        .unwrap()
        .unwrap();

    let updated = Repository {
        name: "renamed".to_string(),
        full_name: "owner/renamed".to_string(),
        archived: true,
        default_branch: Some("trunk".to_string()),
        permissions_snapshot: Some(r#"{"admin":true}"#.to_string()),
        ..existing.clone()
    };
    db.repositories().update(&updated).await.unwrap();

    let reloaded = db
        .repositories()
        .find_by_github_repo_id(1337)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reloaded.id, existing.id, "update must not create a new row");
    assert_eq!(reloaded.name, "renamed");
    assert_eq!(reloaded.full_name, "owner/renamed");
    assert!(reloaded.archived);
    assert_eq!(reloaded.default_branch.as_deref(), Some("trunk"));
    assert!(reloaded.last_synced_at.is_some());

    let all = db.repositories().list_all().await.unwrap();
    assert_eq!(all.len(), 1, "still exactly one repository row");
}
