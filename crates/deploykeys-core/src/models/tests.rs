//! Display/FromStr round-trip coverage for every persisted enum. These
//! conversions define the database TEXT format, so a drifting variant would
//! corrupt rows silently without this safety net.

use super::*;
use std::fmt::Display;
use std::str::FromStr;

fn assert_roundtrip<T>(variants: &[T])
where
    T: Display + FromStr<Err = String> + PartialEq + std::fmt::Debug + Clone,
{
    for variant in variants {
        let text = variant.to_string();
        let parsed =
            T::from_str(&text).unwrap_or_else(|e| panic!("{} did not parse back: {}", text, e));
        assert_eq!(&parsed, variant);
    }
}

#[test]
fn key_algorithm_roundtrip() {
    assert_roundtrip(&[
        KeyAlgorithm::Ed25519,
        KeyAlgorithm::Rsa2048,
        KeyAlgorithm::Rsa4096,
        KeyAlgorithm::EcdsaP256,
        KeyAlgorithm::EcdsaP384,
        KeyAlgorithm::EcdsaP521,
    ]);
    assert!(KeyAlgorithm::from_str("dsa").is_err());
}

#[test]
fn deploy_key_permission_roundtrip() {
    assert_roundtrip(&[
        DeployKeyPermission::ReadOnly,
        DeployKeyPermission::ReadWrite,
    ]);
    assert!(DeployKeyPermission::from_str("admin").is_err());
}

#[test]
fn key_residency_roundtrip() {
    assert_roundtrip(&[KeyResidency::Local, KeyResidency::Remote]);
    assert!(KeyResidency::from_str("cloud").is_err());
}

#[test]
fn key_binding_status_roundtrip() {
    assert_roundtrip(&[
        KeyBindingStatus::Pending,
        KeyBindingStatus::Active,
        KeyBindingStatus::Failed,
        KeyBindingStatus::Drifted,
        KeyBindingStatus::OrphanedLocal,
        KeyBindingStatus::OrphanedRemote,
        KeyBindingStatus::Revoked,
    ]);
    assert!(KeyBindingStatus::from_str("unknown_status").is_err());
}

#[test]
fn auth_type_roundtrip() {
    assert_roundtrip(&[AuthType::GitHubAppDeviceFlow, AuthType::PersonalAccessToken]);
    assert!(AuthType::from_str("oauth_web").is_err());
}

#[test]
fn target_type_roundtrip() {
    assert_roundtrip(&[TargetType::Local, TargetType::Remote]);
    assert!(TargetType::from_str("container").is_err());
}

#[test]
fn os_type_roundtrip() {
    assert_roundtrip(&[
        OsType::MacOs,
        OsType::Linux,
        OsType::Windows,
        OsType::Unknown,
    ]);
    assert!(OsType::from_str("freebsd").is_err());
}

#[test]
fn auth_method_roundtrip() {
    assert_roundtrip(&[AuthMethod::Password, AuthMethod::SshKey]);
    assert!(AuthMethod::from_str("agent").is_err());
}

#[test]
fn target_status_roundtrip() {
    assert_roundtrip(&[
        TargetStatus::Active,
        TargetStatus::Unreachable,
        TargetStatus::AuthFailed,
        TargetStatus::Unknown,
    ]);
    assert!(TargetStatus::from_str("paused").is_err());
}
