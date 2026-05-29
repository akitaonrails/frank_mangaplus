use crate::error::{ApiError, Result};
use crate::proto;
use prost::Message;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Default API host.
pub const API_HOST: &str = "jumpg-api.tokyo-cdn.com";

/// `app_ver` query param we send — pinned to the version we
/// reverse-engineered (v2.3.0 = versionCode 250).
pub const APP_VER: &str = "250";

/// `os_ver` query param. Server doesn't seem to validate the exact value.
pub const OS_VER_DEFAULT: &str = "33";

/// Subset of MANGA Plus language codes we use. The wire format is a
/// language string like "eng"/"esp"/"fra"/etc.; we accept &str for
/// flexibility.
#[allow(dead_code)]
pub mod lang {
    pub const ENGLISH: &str = "eng";
    pub const SPANISH: &str = "esp";
    pub const FRENCH: &str = "fra";
    pub const INDONESIAN: &str = "ind";
    pub const PORTUGUESE_BR: &str = "ptb";
    pub const RUSSIAN: &str = "rus";
    pub const THAI: &str = "tha";
    pub const VIETNAMESE: &str = "vie";
    pub const GERMAN: &str = "deu";
}

/// Builder/config for the API client.
#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub host: String,
    pub app_ver: String,
    pub os_ver: String,
    pub secret: String,
    /// Minimum interval between outbound requests. Defensive against
    /// accidental loops or refresh storms in the UI. Empirically the
    /// MANGA Plus server starts returning generic "Invalid Parameter"
    /// (code 10522) after ~10 requests in a few seconds and locks the
    /// session out for 10+ minutes, so we self-throttle well below that.
    /// Default: 500 ms (2 req/sec max).
    pub min_request_interval: Duration,
}

impl ClientConfig {
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            host: API_HOST.to_string(),
            app_ver: APP_VER.to_string(),
            os_ver: OS_VER_DEFAULT.to_string(),
            secret: secret.into(),
            min_request_interval: Duration::from_millis(500),
        }
    }
}

/// Thin async client. Holds a single `reqwest::Client`, the session
/// secret, and a shared throttle gate. Cheap to clone — the gate is
/// behind an `Arc<Mutex<>>` so clones share the rate-limit state.
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    cfg: ClientConfig,
    last_request_at: Arc<Mutex<Option<Instant>>>,
}

impl Client {
    pub fn new(cfg: ClientConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent("mangaplus-reader/0.1")
            .build()?;
        Ok(Self {
            http,
            cfg,
            last_request_at: Arc::new(Mutex::new(None)),
        })
    }

    /// Block (asynchronously) until enough time has passed since the last
    /// request to respect `min_request_interval`. Holding the mutex across
    /// the sleep serializes burst attempts — if 10 commands fire at once,
    /// they get released one per interval.
    async fn throttle(&self) {
        let mut guard = self.last_request_at.lock().await;
        if let Some(t) = *guard {
            let elapsed = t.elapsed();
            if elapsed < self.cfg.min_request_interval {
                tokio::time::sleep(self.cfg.min_request_interval - elapsed).await;
            }
        }
        *guard = Some(Instant::now());
    }

    /// Issue a request against `/api/{path}`, automatically appending the
    /// four mandatory query params (`os`, `os_ver`, `app_ver`, `secret`)
    /// plus any caller-supplied extras. Returns the raw protobuf body bytes.
    ///
    /// `method` is "GET" / "PUT" / "DELETE" / "POST". The MANGA Plus API
    /// uses query params for everything, so request bodies are always empty.
    pub async fn request_raw(
        &self,
        method: reqwest::Method,
        path: &str,
        extra: &[(&str, &str)],
    ) -> Result<Vec<u8>> {
        self.throttle().await;
        let url = format!("https://{}/api/{}", self.cfg.host, path);
        let mut req = self.http.request(method, &url).query(&[
            ("os", "android"),
            ("os_ver", self.cfg.os_ver.as_str()),
            ("app_ver", self.cfg.app_ver.as_str()),
            ("secret", self.cfg.secret.as_str()),
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

    /// Convenience for the common GET case.
    pub async fn get_raw(&self, path: &str, extra: &[(&str, &str)]) -> Result<Vec<u8>> {
        self.request_raw(reqwest::Method::GET, path, extra).await
    }

    // ---------- typed endpoint wrappers ----------

    pub async fn get_profile(&self) -> Result<proto::ProfileSettingsView> {
        let body = self.get_raw("profile", &[]).await?;
        extract_variant(body, "profile_settings_view", |d| match d {
            proto::success_result::Data::ProfileSettingsView(v) => Ok(v),
            _ => Err(()),
        })
    }

    pub async fn get_favorites(&self) -> Result<proto::SubscribedTitlesView> {
        let body = self.get_raw("title_list/bookmark", &[]).await?;
        extract_variant(body, "subscribed_titles_view", |d| match d {
            proto::success_result::Data::SubscribedTitlesView(v) => Ok(v),
            _ => Err(()),
        })
    }

    /// Add a title to the user's favorites. The server response carries no
    /// payload we care about — we treat 2xx with no error oneof as success.
    pub async fn add_favorite(&self, title_id: u32) -> Result<()> {
        let body = self
            .request_raw(
                reqwest::Method::PUT,
                "title_list/bookmark",
                &[("title_id", &title_id.to_string())],
            )
            .await?;
        decode_ack(body)
    }

    pub async fn remove_favorite(&self, title_id: u32) -> Result<()> {
        let body = self
            .request_raw(
                reqwest::Method::DELETE,
                "title_list/bookmark",
                &[("title_id", &title_id.to_string())],
            )
            .await?;
        decode_ack(body)
    }

    /// Fetch the search catalog for a given language. The server returns
    /// the whole searchable catalog (segmented into `contents`); the caller
    /// is expected to filter by user query string locally.
    pub async fn search(&self, lang: &str, clang: &str) -> Result<proto::SearchView> {
        let body = self
            .get_raw("title_list/search", &[("lang", lang), ("clang", clang)])
            .await?;
        extract_variant(body, "search_view", |d| match d {
            proto::success_result::Data::SearchView(v) => Ok(v),
            _ => Err(()),
        })
    }

    /// Fetch a single title's detail (chapters, overview, banners-stripped).
    ///
    /// `country_code` example: "US", "JP". Empty string also works.
    pub async fn get_title_detail(
        &self,
        title_id: u32,
        lang: &str,
        clang: &str,
        country_code: &str,
    ) -> Result<proto::TitleDetailView> {
        let tid = title_id.to_string();
        let body = self
            .get_raw(
                "title_detailV3",
                &[
                    ("title_id", &tid),
                    ("lang", lang),
                    ("clang", clang),
                    ("country_code", country_code),
                ],
            )
            .await?;
        extract_variant(body, "title_detail_view", |d| match d {
            proto::success_result::Data::TitleDetailView(v) => Ok(v),
            _ => Err(()),
        })
    }

    /// Fetch a chapter's page list.
    ///
    /// `img_quality`: "low" | "high" | "super_high". `viewer_mode`:
    /// "horizontal" | "vertical". The three `*_reading` flags signal the
    /// app's billing context; for a subscribed user, set
    /// `subscription_reading = "yes"` (matches what the app sends).
    pub async fn get_chapter_pages(
        &self,
        chapter_id: u32,
        img_quality: &str,
        viewer_mode: &str,
        clang: &str,
        country_code: &str,
    ) -> Result<proto::MangaViewer> {
        let cid = chapter_id.to_string();
        let body = self
            .get_raw(
                "manga_viewer_v3",
                &[
                    ("chapter_id", &cid),
                    ("split", "yes"),
                    ("img_quality", img_quality),
                    ("ticket_reading", "no"),
                    ("free_reading", "no"),
                    ("subscription_reading", "yes"),
                    ("viewer_mode", viewer_mode),
                    ("clang", clang),
                    ("country_code", country_code),
                ],
            )
            .await?;
        extract_variant(body, "manga_viewer", |d| match d {
            proto::success_result::Data::MangaViewer(v) => Ok(v),
            _ => Err(()),
        })
    }
}

// ---------- shared response-parsing helpers ----------

/// Parse a Response, extract the `success.data` oneof, and apply a
/// caller-supplied matcher that returns Ok(T) for the expected variant.
fn extract_variant<T, F>(body: Vec<u8>, expected_label: &'static str, matcher: F) -> Result<T>
where
    F: FnOnce(proto::success_result::Data) -> std::result::Result<T, ()>,
{
    let resp = proto::Response::decode(&*body)?;
    let data = extract_success_data(resp)?;
    let variant_label = variant_name(&data);
    matcher(data).map_err(|_| ApiError::UnexpectedVariant {
        actual: variant_label,
        expected: expected_label,
    })
}

/// For ack-only responses (PUT/DELETE bookmark): we don't care about the
/// data oneof, only that the response wasn't an error.
fn decode_ack(body: Vec<u8>) -> Result<()> {
    let resp = proto::Response::decode(&*body)?;
    match resp.result {
        Some(proto::response::Result::Success(_)) | None => Ok(()),
        Some(proto::response::Result::Error(e)) => Err(server_error(e)),
    }
}

fn extract_success_data(resp: proto::Response) -> Result<proto::success_result::Data> {
    match resp.result {
        Some(proto::response::Result::Success(s)) => s.data.ok_or(ApiError::EmptyResponse),
        Some(proto::response::Result::Error(e)) => Err(server_error(e)),
        None => Err(ApiError::EmptyResponse),
    }
}

fn server_error(e: proto::ErrorResult) -> ApiError {
    ApiError::Server {
        code: Some(format!("action={}", e.action)),
        action: None,
        english: if e.debug_info.is_empty() { None } else { Some(e.debug_info) },
    }
}

fn variant_name(d: &proto::success_result::Data) -> &'static str {
    use proto::success_result::Data::*;
    match d {
        SubscribedTitlesView(_) => "subscribed_titles_view",
        TitleDetailView(_)      => "title_detail_view",
        MangaViewer(_)          => "manga_viewer",
        ProfileSettingsView(_)  => "profile_settings_view",
        SearchView(_)           => "search_view",
        FavoriteTitlesView(_)   => "favorite_titles_view",
    }
}
