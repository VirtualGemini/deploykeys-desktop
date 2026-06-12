//! Cross-repository integration tests exercising the full foreign-key chain.

use crate::db::test_support::{
    seed_account, seed_installation, seed_repository, seed_target, test_db,
};
use crate::models::{
    DeployKeyPermission, KeyAlgorithm, KeyBinding, KeyBindingStatus, KeyResidency,
};
use chrono::Utc;

#[tokio::test]
async fn installation_chain_is_queryable() {
    let (_dir, db) = test_db().await;
    let account_id = seed_account(&db).await;
    let installation_id = seed_installation(&db, account_id).await;
    let repo_id = seed_repository(&db, installation_id).await;

    let installation = db
        .installations()
        .find_by_id(installation_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(installation.account_id, account_id);

    let by_github_id = db
        .installations()
        .find_by_github_installation_id(9001)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(by_github_id.id, installation_id);

    let installations = db
        .installations()
        .list_by_account(account_id)
        .await
        .unwrap();
    assert_eq!(installations.len(), 1);

    let repos = db
        .repositories()
        .list_by_installation(installation_id)
        .await
        .unwrap();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].id, repo_id);
    assert_eq!(repos[0].full_name, "owner/repo");
    assert!(repos[0].private);
    assert!(!repos[0].archived);
}

#[tokio::test]
async fn repository_lookup_by_github_id_and_sync_time() {
    let (_dir, db) = test_db().await;
    let account_id = seed_account(&db).await;
    let installation_id = seed_installation(&db, account_id).await;
    let repo_id = seed_repository(&db, installation_id).await;

    let repo = db
        .repositories()
        .find_by_github_repo_id(1337)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(repo.id, repo_id);
    assert!(repo.last_synced_at.is_none());

    db.repositories().update_sync_time(repo_id).await.unwrap();

    let synced = db
        .repositories()
        .find_by_id(repo_id)
        .await
        .unwrap()
        .unwrap();
    assert!(synced.last_synced_at.is_some());
}

#[tokio::test]
async fn key_binding_listing_and_update_flow() {
    let (_dir, db) = test_db().await;
    let account_id = seed_account(&db).await;
    let installation_id = seed_installation(&db, account_id).await;
    let repo_id = seed_repository(&db, installation_id).await;
    let target_id = seed_target(&db).await;

    let binding = KeyBinding {
        id: 0,
        repo_id,
        target_id,
        github_deploy_key_id: None,
        deploy_key_title: "integration".to_string(),
        algorithm: KeyAlgorithm::Ed25519,
        permission: DeployKeyPermission::ReadWrite,
        public_key: "ssh-ed25519 AAAA".to_string(),
        public_key_fingerprint: "SHA256:fp".to_string(),
        private_key_path: "/tmp/integration_key".to_string(),
        private_key_residency: KeyResidency::Local,
        status: KeyBindingStatus::Pending,
        created_at: Utc::now(),
        last_verified_at: None,
    };

    let binding_id = db.key_bindings().create(&binding).await.unwrap();

    let by_repo = db.key_bindings().list_by_repo(repo_id).await.unwrap();
    assert_eq!(by_repo.len(), 1);
    assert_eq!(by_repo[0].permission, DeployKeyPermission::ReadWrite);

    let by_target = db.key_bindings().list_by_target(target_id).await.unwrap();
    assert_eq!(by_target.len(), 1);

    let all = db.key_bindings().list_all().await.unwrap();
    assert_eq!(all.len(), 1);

    // Simulate the upload completing: attach the GitHub key id.
    let mut uploaded = by_repo.into_iter().next().unwrap();
    uploaded.github_deploy_key_id = Some(4242);
    uploaded.status = KeyBindingStatus::Active;
    uploaded.last_verified_at = Some(Utc::now());
    db.key_bindings().update(&uploaded).await.unwrap();

    let reloaded = db
        .key_bindings()
        .find_by_id(binding_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reloaded.github_deploy_key_id, Some(4242));
    assert_eq!(reloaded.status, KeyBindingStatus::Active);

    db.key_bindings().delete(binding_id).await.unwrap();
    assert!(db
        .key_bindings()
        .find_by_id(binding_id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn target_update_persists_health_state() {
    use crate::models::TargetStatus;

    let (_dir, db) = test_db().await;
    let target_id = seed_target(&db).await;

    let mut target = db.targets().find_by_id(target_id).await.unwrap().unwrap();
    target.status = TargetStatus::Unreachable;
    target.host_key_fingerprint = Some("SHA256:hostkey".to_string());
    target.last_checked_at = Some(Utc::now());
    db.targets().update(&target).await.unwrap();

    let reloaded = db.targets().find_by_id(target_id).await.unwrap().unwrap();
    assert_eq!(reloaded.status, TargetStatus::Unreachable);
    assert_eq!(
        reloaded.host_key_fingerprint.as_deref(),
        Some("SHA256:hostkey")
    );
    assert!(reloaded.last_checked_at.is_some());

    let listed = db.targets().list_all().await.unwrap();
    assert_eq!(listed.len(), 1);
}
