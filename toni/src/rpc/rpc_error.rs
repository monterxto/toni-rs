use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum RpcError {
    #[error("Pattern not found: {0}")]
    PatternNotFound(String),
    #[error("Guard rejected message: {0}")]
    Forbidden(String),
    #[error("Internal error: {0}")]
    Internal(String),
}
