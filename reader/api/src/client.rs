use crate::error::{ApiError, Result};

/// Default API host. Lives on the regular HTTPS port.
pub const API_HOST: &str = "jumpg-api.tokyo-cdn.com";

/// `app_ver` query param the app sends — pinned to the version we
/// reverse-engineered (v2.3.0 = versionCode 250). If Shueisha tightens
/// this server-side in future, bump it after re-recon.
pub const APP_VER: &str = "250";

/// `os_ver` query param. Server doesn't seem to validate the exact value,
/// just expects something integer-looking.
pub const OS_VER_DEFAULT: &str = "33";

/// Builder/config for the API client.
pub struct ClientConfig {
    pub host: String,
    pub app_ver: String,
    pub os_ver: String,
    pub secret: String,
}

impl ClientConfig {
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            host: API_HOST.to_string(),
            app_ver: APP_VER.to_string(),
            os_ver: OS_VER_DEFAULT.to_string(),
            secret: secret.into(),
        }
    }
}

/// Thin async client. Holds a single `reqwest::Client` and the session secret.
pub struct Client {
    http: reqwest::Client,
    cfg: ClientConfig,
}

impl Client {
    pub fn new(cfg: ClientConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent("mangaplus-reader/0.1")
            .build()?;
        Ok(Self { http, cfg })
    }

    /// Issue a GET against `/api/{path}`, automatically appending the four
    /// mandatory query params (`os`, `os_ver`, `app_ver`, `secret`) plus any
    /// caller-supplied extras. Returns the raw protobuf body bytes.
    pub async fn get_raw(&self, path: &str, extra: &[(&str, &str)]) -> Result<Vec<u8>> {
        let url = format!("https://{}/api/{}", self.cfg.host, path);
        let mut req = self.http.get(&url).query(&[
            ("os", "android"),
            ("os_ver", &self.cfg.os_ver),
            ("app_ver", &self.cfg.app_ver),
            ("secret", &self.cfg.secret),
        ]);
        if !extra.is_empty() {
            req = req.query(extra);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(ApiError::Status(status.as_u16()));
        }
        let body = resp.bytes().await?.to_vec();
        Ok(body)
    }

    // High-level typed wrappers (get_profile, get_favorites, etc.) will be
    // added as the protobuf schemas are transcribed.
}
