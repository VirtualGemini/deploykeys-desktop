use crate::{
    db::Database,
    models::{OsType, Target, TargetStatus, TargetType},
    Result,
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
