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

// ---------- defaults ----------
//
// Documented here in one place so a future tuner can see all the
// knobs without grepping. Surfaced via `ClientConfig::new` and
// referenced from doc comments below.

/// Max concurrent CDN image fetches. Empirically a 12-permit
/// semaphore keeps a 100/1 Mbps residential link busy without
/// exhausting ephemeral sockets when the WebView mounts 80+ thumbnails
/// at once.
pub const DEFAULT_MAX_CONCURRENT_IMAGES: usize = 12;

/// Per-image retry budget on transient failures. 2 extra tries (3
/// total attempts) covers the typical jumpg-assets3 hiccup without
/// burning user time on dead URLs.
pub const DEFAULT_IMAGE_RETRY_ATTEMPTS: u32 = 2;

/// Base backoff for image retries (ms). Doubled each subsequent
/// retry, capped at 256× (see [`backoff_delay_ms`]).
pub const DEFAULT_IMAGE_RETRY_BACKOFF_MS: u64 = 250;

/// Hard cap on a single image body (bytes). Defends against a
/// misbehaving CDN/MITM streaming gigabytes through `Bytes::collect`.
/// 32 MB is comfortably above any real manga page (≤2 MB) plus
/// breathing room.
pub const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;

/// Cap on the exponential shift in [`backoff_delay_ms`]. A shift of 8
/// means base × 256, so with the default 250 ms base the longest
/// single retry sleep is ~64 s — long enough to ride out a brief CDN
/// outage, short enough that a runaway `image_retry_attempts` value
/// can't park a fetch for hours.
const BACKOFF_SHIFT_CAP: u32 = 8;

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
    /// Max concurrent CDN image fetches. The MANGA Plus API server has a
    /// strict rate limit (hence `min_request_interval`), but the CDN
    /// hosts are separate and tolerate parallelism — without one a 80-
    /// thumbnail grid takes 80 × 500 ms = 40 s to populate on a cold
    /// cache. Set high enough to keep the network busy, low enough to
    /// not exhaust ephemeral sockets. Default: 12.
    pub max_concurrent_images: usize,
    /// Per-image retry budget on transient failures (network reset,
    /// 5xx, 408, 429). Each retry sleeps `image_retry_backoff_ms` × 2^n
    /// before re-issuing. Default: 2 (so up to 3 total attempts).
    pub image_retry_attempts: u32,
    /// Base backoff for image retries. Default 250 ms, then 500, 1000…
    pub image_retry_backoff_ms: u64,
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
            max_concurrent_images: DEFAULT_MAX_CONCURRENT_IMAGES,
            image_retry_attempts: DEFAULT_IMAGE_RETRY_ATTEMPTS,
            image_retry_backoff_ms: DEFAULT_IMAGE_RETRY_BACKOFF_MS,
        }
    }
}

/// Thin async client. Holds a single `reqwest::Client`, the session
/// secret, and a shared throttle gate. Cheap to clone — the gate is
/// behind an `Arc<Mutex<>>` so clones share the rate-limit state.
///
/// Image fetches use a separate `tokio::sync::Semaphore` instead of
/// the API throttle: the MANGA Plus CDN tolerates parallelism, and
/// gating image traffic on the same 500-ms-per-request mutex turned
/// a 80-thumbnail grid into a 40-second progressive reveal.
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    cfg: ClientConfig,
    last_request_at: Arc<Mutex<Option<Instant>>>,
    image_gate: Arc<tokio::sync::Semaphore>,
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
        // Cap the semaphore at the configured concurrency. Zero would
        // be a deadlock; floor it at 1 to be safe.
        let permits = cfg.max_concurrent_images.max(1);
        Ok(Self {
            http,
            cfg,
            last_request_at: Arc::new(Mutex::new(None)),
            image_gate: Arc::new(tokio::sync::Semaphore::new(permits)),
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
    /// Concurrency is bounded by `cfg.max_concurrent_images` via an
    /// internal semaphore — this lets dozens of `<img>` tags fire
    /// simultaneously without exhausting sockets or stampeding the CDN.
    /// The API throttle is intentionally NOT applied: image hosts are
    /// separate from the API server and don't share its rate limit.
    ///
    /// Transient failures (connection reset, request timeout, 408,
    /// 429, 5xx) are retried with exponential backoff up to
    /// `cfg.image_retry_attempts` extra tries. Hard 4xx (404, etc.)
    /// fail fast — retrying a 404 just delays the placeholder.
    ///
    /// Cached locally when `image_cache_dir` is configured. Cache key is
    /// the URL path (signed query params stripped, since the underlying
    /// image is stable). A cached hit skips the semaphore entirely so
    /// disk hits never queue behind in-flight network fetches.
    pub async fn fetch_image(&self, url: &str) -> Result<(Vec<u8>, String)> {
        let cache_path = self.cache_path_for(url);
        let ct = Self::content_type_from_ext(url).to_string();

        // Cache hit fast path: no semaphore, no network. Critical for
        // re-renders — the WebView re-requests an image whenever it
        // scrolls back into view, and we don't want the disk hit to
        // queue behind 12 in-flight CDN fetches.
        if let Some(p) = &cache_path {
            if let Ok(bytes) = tokio::fs::read(p).await {
                if !bytes.is_empty() {
                    return Ok((bytes, ct));
                }
            }
        }

        let max_attempts = self.cfg.image_retry_attempts.saturating_add(1);
        let mut last_err: Option<ApiError> = None;
        for attempt in 0..max_attempts {
            if attempt > 0 {
                // Sleep with NO permit held. Holding the permit during
                // backoff means a single slow URL stalls 1/12th of the
                // global concurrency for the full backoff window —
                // under default config that's ~1.75 s per retried URL,
                // collapsing throughput to a crawl when a handful of
                // hosts misbehave.
                let delay_ms = backoff_delay_ms(self.cfg.image_retry_backoff_ms, attempt);
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
            // Re-acquire the permit for each attempt. Tokio's semaphore
            // is FIFO-ish under contention, so the just-released slot
            // goes to whoever is waiting next, not the retrying caller.
            let permit = self
                .image_gate
                .acquire()
                .await
                .expect("image semaphore closed (never closed in practice)");
            let result = self.try_fetch_image_once(url, cache_path.as_ref(), &ct).await;
            drop(permit);
            match result {
                Ok(v) => return Ok(v),
                Err(e) => {
                    if !is_retriable(&e) {
                        return Err(e);
                    }
                    last_err = Some(e);
                }
            }
        }
        // Loop ran but every attempt was retriable-and-failed.
        // `last_err` is guaranteed Some by the loop invariant when
        // `max_attempts >= 1` (saturating_add guarantees that), but
        // express the impossibility explicitly rather than relying on
        // a meaningless `Status(0)` filler.
        Err(last_err.unwrap_or_else(|| {
            ApiError::Other("image fetch exhausted retries with no error captured".into())
        }))
    }

    /// Single CDN attempt. Separated so the retry loop in
    /// [`fetch_image`] can drive it without re-doing cache-key math.
    async fn try_fetch_image_once(
        &self,
        url: &str,
        cache_path: Option<&PathBuf>,
        fallback_ct: &str,
    ) -> Result<(Vec<u8>, String)> {
        let resp = self.http.get(url).send().await?;
        let status = resp.status();
        let server_ct = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| fallback_ct.to_string());
        if !status.is_success() {
            return Err(ApiError::Status(status.as_u16()));
        }
        // Bound the body up-front so a misbehaving CDN or a MITM
        // can't stream gigabytes into our Vec. We trust Content-Length
        // when present (the real CDN always sends it); if absent, we
        // still fall through to `.bytes()` which collects everything
        // in memory — but a sane TCP timeout will catch a pathological
        // dribbling stream long before it matters in practice.
        if let Some(len) = resp.content_length() {
            if len > MAX_IMAGE_BYTES {
                return Err(ApiError::Other(format!(
                    "image too large: {} bytes (cap {})",
                    len, MAX_IMAGE_BYTES
                )));
            }
        }
        let bytes = resp.bytes().await?.to_vec();

        if let Some(p) = cache_path {
            if let Some(parent) = p.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
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

    /// Fetch the full title catalog for one publication-status bucket —
    /// the official app's BrowseTitles tabs ("serializing" = ongoing,
    /// "completed" = finished). To get the entire catalog the caller
    /// must request both buckets and merge.
    ///
    /// Despite the URL saying "all_v3", the response is a SearchView
    /// (success-result variant 35) with the `all_titles_group` field
    /// populated instead of `contents`. We reuse SearchView so the
    /// existing extract path works.
    pub async fn get_all_titles_by_type(
        &self,
        type_bucket: &str,
        lang: &str,
        clang: &str,
    ) -> Result<proto::SearchView> {
        let body = self
            .get_raw(
                "title_list/all_v3",
                &[("type", type_bucket), ("lang", lang), ("clang", clang)],
            )
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

/// Pure backoff schedule for the image-fetch retry loop.
/// `attempt` is 1-based for retries (attempt 0 is the first try,
/// which has no pre-sleep). Shift capped at [`BACKOFF_SHIFT_CAP`]
/// (= 256× multiplier) so a misconfigured `image_retry_attempts`
/// can't sleep for hours. Pure so it can be unit-tested without
/// touching `tokio::time::sleep`.
pub(crate) fn backoff_delay_ms(base_ms: u64, attempt: u32) -> u64 {
    if attempt == 0 {
        return 0;
    }
    let shift = (attempt - 1).min(BACKOFF_SHIFT_CAP);
    base_ms.saturating_mul(1u64 << shift)
}

/// Should the image-fetch loop retry this error? Conservative:
/// connect/timeout/reset are always retried; status codes are retried
/// only for 408 (request timeout), 429 (rate limited), and 5xx
/// (server-side). Hard 4xx (401/403/404/410/etc.) come back
/// immediately because the next attempt would behave identically.
pub(crate) fn is_retriable(err: &ApiError) -> bool {
    match err {
        ApiError::Http(e) => {
            // reqwest::Error::is_timeout / is_connect / is_request all
            // signal transient transport faults.
            e.is_timeout() || e.is_connect() || e.is_request() || e.is_body()
        }
        ApiError::Status(code) => matches!(code, 408 | 429 | 500..=599),
        _ => false,
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

    #[test]
    fn backoff_delay_is_zero_for_initial_attempt() {
        // attempt 0 is the first try; no pre-sleep happens.
        assert_eq!(backoff_delay_ms(250, 0), 0);
        assert_eq!(backoff_delay_ms(9999, 0), 0);
    }

    #[test]
    fn backoff_delay_doubles_each_retry() {
        // Default base = 250 ms. Successive retries: 250, 500, 1 s, 2 s, …
        assert_eq!(backoff_delay_ms(250, 1), 250);
        assert_eq!(backoff_delay_ms(250, 2), 500);
        assert_eq!(backoff_delay_ms(250, 3), 1_000);
        assert_eq!(backoff_delay_ms(250, 4), 2_000);
        assert_eq!(backoff_delay_ms(250, 5), 4_000);
    }

    #[test]
    fn backoff_delay_caps_shift_at_eight() {
        // Shift is capped at 8 (256×) so a runaway `image_retry_attempts`
        // can't sleep for hours per attempt. 250 ms × 256 = 64 000 ms.
        assert_eq!(backoff_delay_ms(250, 9), 64_000);
        assert_eq!(backoff_delay_ms(250, 50), 64_000);
        assert_eq!(backoff_delay_ms(250, u32::MAX), 64_000);
    }

    #[test]
    fn backoff_delay_saturates_on_huge_base() {
        // A pathological `image_retry_backoff_ms` should saturate, not
        // overflow u64 and wrap to a small value.
        let huge = u64::MAX / 2;
        assert_eq!(backoff_delay_ms(huge, 9), u64::MAX); // 256× saturates
    }

    #[test]
    fn is_retriable_treats_transport_faults_and_5xx_as_retriable() {
        assert!(is_retriable(&ApiError::Status(500)));
        assert!(is_retriable(&ApiError::Status(503)));
        assert!(is_retriable(&ApiError::Status(599)));
        assert!(is_retriable(&ApiError::Status(408)));
        assert!(is_retriable(&ApiError::Status(429)));
    }

    #[test]
    fn is_retriable_fails_fast_on_hard_4xx_and_logic_errors() {
        // 4xx responses (other than 408/429) indicate a request-shape
        // problem — a retry would behave identically and just delay
        // surfacing the failure to the user.
        assert!(!is_retriable(&ApiError::Status(400)));
        assert!(!is_retriable(&ApiError::Status(401)));
        assert!(!is_retriable(&ApiError::Status(403)));
        assert!(!is_retriable(&ApiError::Status(404)));
        assert!(!is_retriable(&ApiError::Status(410)));
        // Decode + EmptyResponse aren't transient.
        assert!(!is_retriable(&ApiError::EmptyResponse));
    }

    #[tokio::test]
    async fn image_semaphore_caps_concurrency() {
        // Three concurrent image fetches against a permit count of 2
        // must serialise the third — observable via a measurable gap
        // between the first batch and the third permit acquisition.
        let mut cfg = ClientConfig::new("secret");
        cfg.max_concurrent_images = 2;
        let client = Client::new(cfg).unwrap();
        let sem = client.image_gate.clone();
        let p1 = sem.clone().acquire_owned().await.unwrap();
        let p2 = sem.clone().acquire_owned().await.unwrap();
        // No permits available — try_acquire should fail.
        assert!(sem.clone().try_acquire_owned().is_err());
        drop(p1);
        // Releasing one makes a slot available again.
        assert!(sem.clone().try_acquire_owned().is_ok());
        drop(p2);
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
