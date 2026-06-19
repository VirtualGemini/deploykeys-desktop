use crate::{
    credentials::CredentialStore,
    models::{AuthMethod, Target, TargetType},
    Error, Result,
};
use async_trait::async_trait;
use russh::{client, ChannelMsg, Disconnect};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RemoteCommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub async fn run_remote_command(target: &Target, command: &str) -> Result<RemoteCommandOutput> {
    if target.target_type != TargetType::Remote {
        return Err(Error::Validation("Target is not remote".into()));
    }

    match target.auth_method {
        Some(AuthMethod::Password) => run_password_remote_command(target, command).await,
        Some(AuthMethod::SshKey) | None => run_key_remote_command(target, command).await,
    }
}

async fn run_key_remote_command(target: &Target, command: &str) -> Result<RemoteCommandOutput> {
    let host = target
        .host
        .as_deref()
        .ok_or_else(|| Error::Validation("Remote host is required".into()))?;
    let username = target
        .username
        .as_deref()
        .ok_or_else(|| Error::Validation("Remote username is required".into()))?;
    let key_path = target
        .auth_ref
        .as_deref()
        .ok_or_else(|| Error::Validation("Remote SSH private key path is required".into()))?;
    let port = target.port.unwrap_or(22).to_string();
    let destination = format!("{username}@{host}");

    let output = tokio::process::Command::new("ssh")
        .arg("-i")
        .arg(key_path)
        .arg("-p")
        .arg(port)
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("IdentitiesOnly=yes")
        .arg("-o")
        .arg("ConnectTimeout=10")
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg(destination)
        .arg(command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| Error::Ssh(format!("Failed to start ssh: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if output.status.success() {
        return Ok(RemoteCommandOutput {
            stdout,
            stderr,
            exit_code: output.status.code().unwrap_or(0),
        });
    }

    let detail = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    let status = output
        .status
        .code()
        .map(|code| code.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    Err(Error::Ssh(format!(
        "Remote command failed with status {status}: {}",
        if detail.is_empty() {
            "No output"
        } else {
            detail
        }
    )))
}

struct PasswordClient;

#[async_trait]
impl client::Handler for PasswordClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::key::PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        Ok(true)
    }
}

async fn run_password_remote_command(
    target: &Target,
    command: &str,
) -> Result<RemoteCommandOutput> {
    let host = target
        .host
        .as_deref()
        .ok_or_else(|| Error::Validation("Remote host is required".into()))?;
    let username = target
        .username
        .as_deref()
        .ok_or_else(|| Error::Validation("Remote username is required".into()))?;
    let password_ref = target
        .auth_ref
        .as_deref()
        .ok_or_else(|| Error::Validation("Remote SSH password reference is required".into()))?;
    let password_ref = password_ref.to_string();
    let password =
        tokio::task::spawn_blocking(move || CredentialStore::get_ssh_password(&password_ref))
            .await
            .map_err(|e| Error::Other(format!("Keyring task failed: {e}")))??;
    let port = target.port.unwrap_or(22);

    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(30)),
        keepalive_interval: Some(Duration::from_secs(10)),
        ..Default::default()
    });
    let address = (host, port);
    let mut session = tokio::time::timeout(
        Duration::from_secs(15),
        client::connect(config, address, PasswordClient),
    )
    .await
    .map_err(|_| Error::Ssh("SSH connection timed out".into()))?
    .map_err(|e| Error::Ssh(format!("SSH connection failed: {e}")))?;

    let authenticated = session
        .authenticate_password(username, password.clone())
        .await
        .map_err(|e| Error::Ssh(format!("SSH password authentication failed: {e}")))?;
    if !authenticated
        && !authenticate_keyboard_interactive(&mut session, username, &password).await?
    {
        return Err(Error::Auth("SSH password authentication failed".into()));
    }

    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| Error::Ssh(format!("Failed to open SSH session: {e}")))?;
    channel
        .exec(true, command)
        .await
        .map_err(|e| Error::Ssh(format!("Failed to execute remote command: {e}")))?;

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut exit_code = None::<i32>;
    while let Some(message) = channel.wait().await {
        match message {
            ChannelMsg::Data { data } => stdout.extend_from_slice(data.as_ref()),
            ChannelMsg::ExtendedData { data, ext } if ext == 1 => {
                stderr.extend_from_slice(data.as_ref())
            }
            ChannelMsg::ExitStatus { exit_status } => {
                exit_code = Some(i32::try_from(exit_status).unwrap_or(i32::MAX));
            }
            ChannelMsg::Close => break,
            _ => {}
        }
    }

    let _ = session
        .disconnect(Disconnect::ByApplication, "", "English")
        .await;
    let stdout = String::from_utf8_lossy(&stdout).to_string();
    let stderr = String::from_utf8_lossy(&stderr).to_string();
    let exit_code = exit_code.unwrap_or(0);
    if exit_code == 0 {
        return Ok(RemoteCommandOutput {
            stdout,
            stderr,
            exit_code,
        });
    }

    let detail = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    Err(Error::Ssh(format!(
        "Remote command failed with status {exit_code}: {}",
        if detail.is_empty() {
            "No output"
        } else {
            detail
        }
    )))
}

async fn authenticate_keyboard_interactive(
    session: &mut client::Handle<PasswordClient>,
    username: &str,
    password: &str,
) -> Result<bool> {
    let mut state = session
        .authenticate_keyboard_interactive_start(username, Some(String::new()))
        .await
        .map_err(|e| {
            Error::Ssh(format!(
                "SSH keyboard-interactive authentication failed: {e}"
            ))
        })?;

    for _ in 0..5 {
        match state {
            client::KeyboardInteractiveAuthResponse::Success => return Ok(true),
            client::KeyboardInteractiveAuthResponse::Failure => return Ok(false),
            client::KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                let responses = prompts
                    .into_iter()
                    .map(|prompt| {
                        let prompt_text = prompt.prompt.to_ascii_lowercase();
                        if !prompt.echo && prompt_text.contains("password") {
                            password.to_string()
                        } else {
                            String::new()
                        }
                    })
                    .collect::<Vec<_>>();
                state = session
                    .authenticate_keyboard_interactive_respond(responses)
                    .await
                    .map_err(|e| {
                        Error::Ssh(format!(
                            "SSH keyboard-interactive authentication failed: {e}"
                        ))
                    })?;
            }
        }
    }

    Ok(false)
}

pub fn remote_private_key_path(target: &Target, directory: &str, algorithm: &str) -> String {
    join_remote_path(
        &join_remote_path(&target.key_base_dir, directory),
        &format!("id_{algorithm}"),
    )
}

pub fn join_remote_path(base: &str, child: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.is_empty() {
        child.to_string()
    } else {
        format!("{base}/{}", child.trim_start_matches('/'))
    }
}

pub fn dirname_remote_path(path: &str) -> Option<String> {
    path.rsplit_once('/').map(|(dir, _)| dir.to_string())
}

pub fn quote_shell(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}
