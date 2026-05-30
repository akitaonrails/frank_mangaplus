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
    // The generated code can't be edited; silence lints that fire on it.
    // `large_enum_variant` triggers because Response's success variant
    // (a TitleDetailView etc.) is much bigger than the error variant.
    // Boxing isn't worth the API churn at the size of data we're dealing with.
    #![allow(clippy::large_enum_variant)]
    #![allow(clippy::derive_partial_eq_without_eq)]
    include!(concat!(env!("OUT_DIR"), "/mangaplus.rs"));
}

mod client;
mod error;

pub use client::{lang, register_new_device, Client, ClientConfig, API_HOST, APP_VER, OS_VER_DEFAULT};
pub use error::{ApiError, Result};
