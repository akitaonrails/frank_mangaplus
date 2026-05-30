use thiserror::Error;

pub type Result<T> = std::result::Result<T, ApiError>;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("HTTP transport: {0}")]
    Http(#[from] reqwest::Error),

    #[error("protobuf decode: {0}")]
    Decode(#[from] prost::DecodeError),

    #[error("server returned non-2xx status: {0}")]
    Status(u16),

    #[error("server response had neither success nor error payload")]
    EmptyResponse,

    #[error("API error: code={code:?} action={action:?} english={english:?}")]
    Server {
        code: Option<String>,
        action: Option<String>,
        english: Option<String>,
    },

    #[error("unexpected payload variant (got {actual}, expected {expected})")]
    UnexpectedVariant {
        actual: &'static str,
        expected: &'static str,
    },

    #[error("{0}")]
    Other(String),
}
