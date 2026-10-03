use serde::{Deserialize, Serialize};
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
pub fn fail<T>(message: impl Into<String>) -> Result<T> {
    Err(Error::Message(message.into()))
}
pub fn timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeType {
    Php,
    Mysql,
    Caddy,
    Node,
    Other(String),
}
impl RuntimeType {
    pub fn key(&self) -> &str {
        match self {
            Self::Php => "php",
            Self::Mysql => "mysql",
            Self::Caddy => "caddy",
            Self::Node => "node",
            Self::Other(v) => v,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeRef {
    pub kind: RuntimeType,
    pub version: String,
}
impl RuntimeRef {
    pub fn key(&self) -> String {
        format!("{}:{}", self.kind.key(), self.version)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceState {
    pub key: String,
    pub pid: Option<u32>,
    pub status: String,
    pub port: Option<u16>,
    pub healthy: bool,
    pub log: String,
}
