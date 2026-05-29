use mangaplus_api::{proto, Client, ClientConfig};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::http::Response;

/// Shared state held by Tauri and handed to every command invocation.
/// The client is wrapped in a Mutex so we can swap it out when the user
/// pastes a new secret via the setup dialog without restarting the app.
struct AppState {
    client: std::sync::Mutex<Arc<Client>>,
}

fn rebuild_client(secret: &str) -> Result<Arc<Client>, String> {
    let mut cfg = ClientConfig::new(secret.to_string());
    cfg.image_cache_dir = Some(image_cache_dir());
    Client::new(cfg)
        .map(Arc::new)
        .map_err(|e| format!("rebuild client: {e}"))
}

#[tauri::command]
fn is_configured(state: tauri::State<'_, AppState>) -> bool {
    state
        .client
        .lock()
        .ok()
        .map(|c| !c.config().secret.is_empty())
        .unwrap_or(false)
}

#[tauri::command]
async fn set_secret(
    state: tauri::State<'_, AppState>,
    scheme_client: tauri::State<'_, SchemeClientState>,
    value: String,
) -> Result<(), String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        return Err("empty secret".into());
    }
    // Persist to disk first; if that fails the in-memory client stays as-is.
    let path = secret_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create config dir: {e}"))?;
    }
    std::fs::write(&path, &trimmed).map_err(|e| format!("write secret file: {e}"))?;
    // Rebuild the API client with the new secret, applied to BOTH the
    // commands' client and the mpimg:// scheme handler's client.
    let new_client = rebuild_client(&trimmed)?;
    {
        let mut g = state.client.lock().map_err(|e| format!("state lock: {e}"))?;
        *g = new_client.clone();
    }
    {
        let mut g = scheme_client
            .0
            .lock()
            .map_err(|e| format!("scheme lock: {e}"))?;
        *g = new_client;
    }
    Ok(())
}

/// Wrapper so we can `.manage()` the scheme handler's Arc<Mutex<Arc<Client>>>
/// independently of `AppState`.
struct SchemeClientState(Arc<std::sync::Mutex<Arc<Client>>>);

/// XDG config dir holding the on-disk secret file fallback.
/// Linux/macOS: ~/.config/mangaplus-reader/secret
/// Windows:     %APPDATA%/mangaplus-reader/secret
fn config_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("mangaplus-reader");
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        return PathBuf::from(appdata).join("mangaplus-reader");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".config/mangaplus-reader");
    }
    std::env::temp_dir().join("mangaplus-reader")
}

fn secret_file() -> PathBuf {
    config_dir().join("secret")
}

/// Look for a configured secret. Env var wins; falls back to the on-disk
/// config file. Returns an empty string if neither has a usable value —
/// the app launches anyway, the frontend shows a setup dialog.
fn read_secret() -> String {
    if let Ok(s) = std::env::var("MANGAPLUS_SECRET") {
        let s = s.trim().to_string();
        if !s.is_empty() {
            return s;
        }
    }
    if let Ok(s) = std::fs::read_to_string(secret_file()) {
        return s.trim().to_string();
    }
    String::new()
}

/// XDG cache dir for the app's image cache. Falls back to ~/.cache then
/// to a tempdir. Created on first write by fetch_image.
fn image_cache_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg).join("mangaplus-reader");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".cache/mangaplus-reader");
    }
    std::env::temp_dir().join("mangaplus-reader")
}

// ---------- commands ----------
//
// Each typed command takes `tauri::State<AppState>` and returns
// `Result<T, String>` where T is a prost-generated proto type that has
// `#[derive(serde::Serialize)]` injected by api/build.rs.
//
// Tauri serializes the result through serde_json; the Svelte frontend
// receives camelCase JSON because of the type_attribute in build.rs.

/// Clone the current Arc<Client> out from under the Mutex, releasing the
/// lock before we hit any `.await`. Holding a `std::sync::MutexGuard`
/// across an await suspends a non-Send future and Tauri rejects it.
fn clone_client(state: &tauri::State<'_, AppState>) -> Result<Arc<Client>, String> {
    let guard = state.client.lock().map_err(|e| format!("state lock: {e}"))?;
    Ok(guard.clone())
}

#[tauri::command]
async fn get_profile(
    state: tauri::State<'_, AppState>,
) -> Result<proto::ProfileSettingsView, String> {
    let client = clone_client(&state)?;
    client.get_profile().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_favorites(
    state: tauri::State<'_, AppState>,
) -> Result<proto::SubscribedTitlesView, String> {
    let client = clone_client(&state)?;
    client.get_favorites().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn add_favorite(
    state: tauri::State<'_, AppState>,
    title_id: u32,
) -> Result<(), String> {
    let client = clone_client(&state)?;
    client.add_favorite(title_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn remove_favorite(
    state: tauri::State<'_, AppState>,
    title_id: u32,
) -> Result<(), String> {
    let client = clone_client(&state)?;
    client.remove_favorite(title_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn search(
    state: tauri::State<'_, AppState>,
    lang: String,
    clang: String,
) -> Result<proto::SearchView, String> {
    let client = clone_client(&state)?;
    client.search(&lang, &clang).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_title_detail(
    state: tauri::State<'_, AppState>,
    title_id: u32,
    lang: String,
    clang: String,
    country_code: String,
) -> Result<proto::TitleDetailView, String> {
    let client = clone_client(&state)?;
    client
        .get_title_detail(title_id, &lang, &clang, &country_code)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_chapter_pages(
    state: tauri::State<'_, AppState>,
    chapter_id: u32,
    img_quality: String,
    viewer_mode: String,
    clang: String,
    country_code: String,
) -> Result<proto::MangaViewer, String> {
    let client = clone_client(&state)?;
    client
        .get_chapter_pages(chapter_id, &img_quality, &viewer_mode, &clang, &country_code)
        .await
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let secret = read_secret();
    eprintln!(
        "[mangaplus-reader] image cache: {}",
        image_cache_dir().display()
    );
    if secret.is_empty() {
        eprintln!("[mangaplus-reader] no deviceSecret configured — app will show setup dialog");
    }
    let client = rebuild_client(&secret).expect("build api client");
    let state = AppState {
        client: std::sync::Mutex::new(client.clone()),
    };
    // The custom URI scheme also wants Arc<Client>. Tracking it via the
    // same Mutex means a paste-the-secret reload swaps the client used by
    // image fetches too.
    let scheme_state: Arc<std::sync::Mutex<Arc<Client>>> =
        Arc::new(std::sync::Mutex::new(client.clone()));
    let scheme_state_for_handler = scheme_state.clone();
    // Also keep the scheme's view in sync when set_secret runs — done by
    // storing the same Arc inside AppState below. We can't share one
    // Mutex between AppState (managed) and the scheme closure without an
    // additional indirection, so we use two Mutexes that we update from
    // set_secret. Slight inelegance, but keeps the types straightforward.

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // mpimg:// custom protocol — proxies image fetches through our Rust
        // client so the plus_vw_token cookie (issued on the API call)
        // threads through to the CDN. Without this proxy the WebView's
        // <img> tags hit jumpg-assets3 cookieless and get 400. The CDN
        // also rejects non-OkHttp-looking User-Agents, which our client
        // sets to "okhttp/4.12.0".
        //
        // Frontend usage: replace the `https://` of imageUrl with `mpimg://`.
        .register_asynchronous_uri_scheme_protocol("mpimg", move |_ctx, request, responder| {
            let url = request.uri().to_string();
            let https_url = url.replacen("mpimg://", "https://", 1);
            let client = scheme_state_for_handler
                .lock()
                .map(|g| g.clone())
                .unwrap_or_else(|_| Arc::new(Client::new(ClientConfig::new("")).unwrap()));
            tauri::async_runtime::spawn(async move {
                let resp = match client.fetch_image(&https_url).await {
                    Ok((bytes, ct)) => Response::builder()
                        .header("Content-Type", ct)
                        // tell the WebView it can cache aggressively;
                        // CDN URLs are signed and effectively immutable.
                        .header("Cache-Control", "public, max-age=86400")
                        .body(bytes)
                        .unwrap_or_else(|_| Response::new(b"build-resp-err".to_vec())),
                    Err(e) => {
                        eprintln!("[mpimg] fetch failed for {https_url}: {e}");
                        Response::builder()
                            .status(500)
                            .body(format!("fetch error: {e}").into_bytes())
                            .unwrap_or_else(|_| Response::new(b"err".to_vec()))
                    }
                };
                responder.respond(resp);
            });
        })
        .manage(state)
        .manage(SchemeClientState(scheme_state))
        .invoke_handler(tauri::generate_handler![
            is_configured,
            set_secret,
            get_profile,
            get_favorites,
            add_favorite,
            remove_favorite,
            search,
            get_title_detail,
            get_chapter_pages,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
