use crate::error::{ApiError, Result};
use crate::proto;
use prost::Message;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Default API host.
pub const API_HOST: &str = "jumpg-api.tokyo-cdn.com";

/// `app_ver` query param we send — pinned to the version we
/// reverse-engineered (v2.3.0 = versionCode 250).
pub const APP_VER: &str = "250";

/// `os_ver` query param. The Android `Build.VERSION.SDK_INT` value the
/// app sends — set to match what the AVD's MANGA Plus actually transmits.
/// Empirical: `33` works for the catalog/profile endpoints but the
/// premium image CDN (jumpg-assets3) returns 400 for old values. The
/// AVD running API 36 (Android 16) sends `36` and the CDN accepts it.
pub const OS_VER_DEFAULT: &str = "36";

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
    /// Where to cache fetched image bytes. `None` disables caching. The
    /// directory is created on demand; URL path becomes the cache layout
    /// (e.g. `<cache_dir>/title/100020/chapter/1000486/manga_page/high/1.webp`).
    pub image_cache_dir: Option<PathBuf>,
}

impl ClientConfig {
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            host: API_HOST.to_string(),
            app_ver: APP_VER.to_string(),
            os_ver: OS_VER_DEFAULT.to_string(),
            secret: secret.into(),
            min_request_interval: Duration::from_millis(500),
            image_cache_dir: None,
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
    /// Read-only view of the config — handy for callers that need to know
    /// whether a secret was provided, etc.
    pub fn config(&self) -> &ClientConfig {
        &self.cfg
    }

    pub fn new(cfg: ClientConfig) -> Result<Self> {
        // CRITICAL: cookies must be enabled. The MANGA Plus API issues a
        // `plus_vw_token` cookie on manga_viewer_v3 responses (domain
        // .tokyo-cdn.com, SameSite=None, HttpOnly) that must be sent on
        // every subsequent image fetch from jumpg-assets3, or the CDN
        // returns 400. The decompiled app uses an OkHttp CookieJar that
        // stores ALL cookies across hosts — we mirror that here.
        //
        // User-Agent must look like Android OkHttp; arbitrary UAs (e.g.
        // "reqwest/0.12") get the same 400.
        let http = reqwest::Client::builder()
            .user_agent("okhttp/4.12.0")
            .cookie_store(true)
            .build()?;
        Ok(Self {
            http,
            cfg,
            last_request_at: Arc::new(Mutex::new(None)),
        })
    }

    /// Map a CDN image URL to a stable cache path.
    /// "https://host/secure/title/X/chapter/Y/manga_page/high/1.webp?hash=..."
    /// becomes "<cache_dir>/title/X/chapter/Y/manga_page/high/1.webp".
    fn cache_path_for(&self, url: &str) -> Option<PathBuf> {
        let base = self.cfg.image_cache_dir.as_ref()?;
        let no_query = url.split('?').next().unwrap_or(url);
        // Drop "https://host/" by taking everything after the third slash.
        let after_host = no_query.splitn(4, '/').nth(3).unwrap_or(no_query);
        // CDN paths start with "secure/" which we strip for cleanliness.
        let cleaned = after_host.strip_prefix("secure/").unwrap_or(after_host);
        Some(base.join(cleaned))
    }

    fn content_type_from_ext(url: &str) -> &'static str {
        let path = url.split('?').next().unwrap_or(url);
        match path.rsplit_once('.').map(|x| x.1) {
            Some("png")  => "image/png",
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("gif")  => "image/gif",
            Some("avif") => "image/avif",
            Some("heif") | Some("heic") => "image/heif",
            _ => "image/webp", // overwhelmingly the case
        }
    }

    /// Fetch an image from the MANGA Plus CDN. Returns (bytes, content-type).
    /// The cookie acquired on a recent API call (e.g. get_chapter_pages) is
    /// reused automatically via the cookie store.
    ///
    /// Cached locally when `image_cache_dir` is configured. Cache key is the
    /// URL path (signed query params stripped, since the underlying image
    /// is stable). A second request for the same path is served from disk
    /// without touching the network.
    pub async fn fetch_image(&self, url: &str) -> Result<(Vec<u8>, String)> {
        let cache_path = self.cache_path_for(url);
        let ct = Self::content_type_from_ext(url).to_string();

        if let Some(p) = &cache_path {
            if let Ok(bytes) = tokio::fs::read(p).await {
                if !bytes.is_empty() {
                    return Ok((bytes, ct));
                }
            }
        }

        self.throttle().await;
        let resp = self.http.get(url).send().await?;
        let status = resp.status();
        let server_ct = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or(ct);
        if !status.is_success() {
            return Err(ApiError::Status(status.as_u16()));
        }
        let bytes = resp.bytes().await?.to_vec();

        if let Some(p) = &cache_path {
            if let Some(parent) = p.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            // Best effort — don't fail the user-visible fetch if write fails.
            let _ = tokio::fs::write(p, &bytes).await;
        }

        Ok((bytes, server_ct))
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
        RegisterationData(_)    => "registeration_data",
        SubscribedTitlesView(_) => "subscribed_titles_view",
        TitleDetailView(_)      => "title_detail_view",
        MangaViewer(_)          => "manga_viewer",
        ProfileSettingsView(_)  => "profile_settings_view",
        SearchView(_)           => "search_view",
        FavoriteTitlesView(_)   => "favorite_titles_view",
    }
}

// ---------- fresh device registration ----------

/// Salt baked into the official Android app. Combined with MD5(android_id)
/// to derive the security_key the registration endpoint expects. Present
/// in InitialRegistrationViewModel.java:69 of v2.3.0 as a plain string
/// literal — there is no obfuscation, native lib, or runtime decryption.
const REGISTER_SALT: &str = "4Kin9vGg";

/// Register a fresh device with no prior credentials. Returns the
/// `deviceSecret` the server hands back, which is then a valid auth
/// token for every other endpoint. The session has free-tier access
/// only — subscription-locked chapters require a `subscription_restore`
/// or `subscription_receipt` call with a real Google Play purchase
/// signature, which the desktop can't produce.
///
/// On the wire:
///   PUT https://jumpg-api.tokyo-cdn.com/api/register
///     ?os=android&os_ver=…&app_ver=…
///     &device_token=<MD5(android_id)>
///     &security_key=<MD5(device_token + "4Kin9vGg")>
///
/// `android_id` is normally `Settings.Secure.ANDROID_ID`, a 64-bit hex
/// value. We generate 8 random bytes and hex-encode them — the server
/// treats it as an opaque key, no validation against device attestation.
pub async fn register_new_device() -> Result<String> {
    let mut bytes = [0u8; 8];
    getrandom::getrandom(&mut bytes).map_err(|e| ApiError::Other(format!("getrandom: {e}")))?;
    let android_id = hex::encode(bytes);
    let (device_token, security_key) = derive_register_params(&android_id);

    let http = reqwest::Client::builder()
        .user_agent("okhttp/4.12.0")
        .build()?;
    let url = format!("https://{API_HOST}/api/register");
    let resp = http
        .put(&url)
        .query(&[
            ("os", "android"),
            ("os_ver", OS_VER_DEFAULT),
            ("app_ver", APP_VER),
            ("device_token", device_token.as_str()),
            ("security_key", security_key.as_str()),
        ])
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(ApiError::Status(status.as_u16()));
    }
    let body = resp.bytes().await?.to_vec();
    extract_variant(body, "registeration_data", |d| match d {
        proto::success_result::Data::RegisterationData(r) => Ok(r.device_secret),
        _ => Err(()),
    })
}

/// Pure derivation of the two query params the /register endpoint expects.
/// Split out from `register_new_device` so we can pin it in unit tests
/// without hitting the network — a salt or MD5 regression here would
/// silently break self-registration in CI without this test catching it.
fn derive_register_params(android_id: &str) -> (String, String) {
    let device_token = md5_hex(android_id.as_bytes());
    let security_key = md5_hex(format!("{device_token}{REGISTER_SALT}").as_bytes());
    (device_token, security_key)
}

fn md5_hex(input: &[u8]) -> String {
    use md5::{Digest, Md5};
    let mut h = Md5::new();
    h.update(input);
    hex::encode(h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn md5_hex_known_vectors() {
        // RFC-style known test vectors so a broken md5 crate is caught.
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn register_handshake_matches_official_app() {
        // Pin the derivation against a known android_id. If the salt
        // changes or md5_hex breaks, self-register stops working — this
        // test catches it offline, before the next release goes out.
        let android_id = "0123456789abcdef";
        let (device_token, security_key) = derive_register_params(android_id);
        assert_eq!(device_token, "4032af8d61035123906e58e067140cc5");
        assert_eq!(security_key, "1d296d89370e92821e55b10ef8c3b315");
    }

    #[test]
    fn content_type_from_ext_known_extensions() {
        assert_eq!(Client::content_type_from_ext("https://h/p/img.png"),  "image/png");
        assert_eq!(Client::content_type_from_ext("https://h/p/img.jpg"),  "image/jpeg");
        assert_eq!(Client::content_type_from_ext("https://h/p/img.jpeg"), "image/jpeg");
        assert_eq!(Client::content_type_from_ext("https://h/p/img.gif"),  "image/gif");
        assert_eq!(Client::content_type_from_ext("https://h/p/img.avif"), "image/avif");
        assert_eq!(Client::content_type_from_ext("https://h/p/img.heic"), "image/heif");
        assert_eq!(Client::content_type_from_ext("https://h/p/img.webp"), "image/webp");
        // Unknown extension falls back to webp (the overwhelming case).
        assert_eq!(Client::content_type_from_ext("https://h/p/img.xyz"), "image/webp");
        // Query string must not confuse the extension match.
        assert_eq!(
            Client::content_type_from_ext("https://h/p/img.png?sig=abc"),
            "image/png"
        );
    }

    #[test]
    fn cache_path_for_strips_secure_and_query() {
        let mut cfg = ClientConfig::new("secret");
        cfg.image_cache_dir = Some(Path::new("/tmp/cache").to_path_buf());
        let client = Client::new(cfg).unwrap();

        let cached = client
            .cache_path_for(
                "https://jumpg-assets3.tokyo-cdn.com/secure/title/1/chapter/2/manga_page/high/3.webp?hash=abc",
            )
            .unwrap();
        assert_eq!(
            cached,
            Path::new("/tmp/cache/title/1/chapter/2/manga_page/high/3.webp"),
        );
    }

    #[test]
    fn cache_path_for_returns_none_without_cache_dir() {
        let client = Client::new(ClientConfig::new("secret")).unwrap();
        assert!(client
            .cache_path_for("https://host/secure/x.webp")
            .is_none());
    }

    #[tokio::test]
    async fn throttle_enforces_min_interval() {
        let mut cfg = ClientConfig::new("secret");
        cfg.min_request_interval = std::time::Duration::from_millis(40);
        let client = Client::new(cfg).unwrap();

        let start = std::time::Instant::now();
        client.throttle().await;
        client.throttle().await;
        client.throttle().await;
        let elapsed = start.elapsed();

        // Three calls: first is free, next two each wait ~40ms. So total
        // should be ≥ ~80ms. Generous lower bound to avoid CI flakes.
        assert!(
            elapsed >= std::time::Duration::from_millis(70),
            "throttle did not pace: elapsed={elapsed:?}"
        );
    }
}
