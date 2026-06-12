use crate::Result;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[async_trait]
pub trait SshExecutor: Send + Sync {
    async fn connect(&mut self) -> Result<()>;
    async fn exec(&self, command: &str) -> Result<CommandOutput>;
    async fn read_file(&self, path: &str) -> Result<String>;
    async fn disconnect(&mut self) -> Result<()>;
}
