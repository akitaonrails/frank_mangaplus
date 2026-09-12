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

    #[error("API error: {}", server_error_text(code, english))]
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

/// Human-first rendering of a server ErrorResult: lead with the English
/// popup text when the server sent one, keep the action code as a
/// parenthesized suffix for bug reports. Falls back to the bare code so
/// a popup-less error still says *something*.
fn server_error_text(code: &Option<String>, english: &Option<String>) -> String {
    match (english, code) {
        (Some(msg), Some(c)) => format!("{msg} ({c})"),
        (Some(msg), None) => msg.clone(),
        (None, Some(c)) => c.clone(),
        (None, None) => "unknown server error".to_string(),
    }
}
