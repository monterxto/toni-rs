use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum RpcClientError {
    #[error("Transport error: {0}")]
    Transport(String),
    #[error("Request timed out")]
    Timeout,
    /// The remote service returned an error envelope.
    #[error("Remote error ({status}): {message}")]
    Remote { message: String, status: String },
}
