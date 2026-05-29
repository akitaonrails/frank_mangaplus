//! Async client for the MANGA Plus mobile API.
//!
//! Talks to `https://jumpg-api.tokyo-cdn.com/api/` over HTTPS, sending
//! `os`, `os_ver`, `app_ver` and `secret` as query parameters (no headers,
//! no signed bodies). Responses are protobuf-encoded.
//!
//! All public methods are pure functions of `(self, args)`: no env reads,
//! no file IO, no logging. The host app supplies the `deviceSecret` once
//! at construction time.

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/mangaplus.rs"));
}

mod client;
mod error;

pub use client::Client;
pub use error::{ApiError, Result};
