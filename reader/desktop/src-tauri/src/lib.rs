use mangaplus_api::{proto, register_new_device, Client, ClientConfig};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::http::Response;
use tauri::Emitter;

// GPU + display-server detection and render-mode policy.
// Pure logic + tests live in render_env.rs; this module is wired into
// run() to replace the previous unconditional WEBKIT_DISABLE_*
// approach.
#[cfg(target_os = "linux")]
mod render_env;

// SWR cache for /title_list/all_v3 — the full title catalog.
// Pure logic + tests live in all_titles_cache.rs; this module wires
// it into the get_all_titles_cached command.
mod all_titles_cache;

/// Shared state held by Tauri and handed to every command invocation.
/// The client is wrapped in a Mutex so we can swap it out when the user
/// pastes a new secret via the setup dialog without restarting the app.
struct AppState {
    client: std::sync::Mutex<Arc<Client>>,
    /// Per-locale "refresh in flight" guard so a flurry of search-page
    /// mounts doesn't fan into N parallel /all_v3 fetches.
    refresh_guards: Arc<all_titles_cache::RefreshGuards>,
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

/// True when the current secret on disk was written by auto-register and
/// the user hasn't yet acknowledged that they're on a free-tier session.
/// The frontend uses this to surface a one-time upgrade-or-continue prompt.
#[tauri::command]
fn is_auto_registered() -> bool {
    auto_register_flag_file().exists()
}

/// Dismiss the free-tier prompt without changing the secret. Subsequent
/// launches won't re-show the prompt unless the user clears the secret
/// (forcing a fresh auto-register).
#[tauri::command]
fn acknowledge_free_tier() -> Result<(), String> {
    let path = auto_register_flag_file();
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("clear flag: {e}"))?;
    }
    Ok(())
}

/// Frontend handshake — called from the SvelteKit layout on first
/// successful render. Clears the recovery marker that was touched at
/// startup; if the WebView aborts before this fires, the marker stays
/// behind and the NEXT launch reads it as "previous run died" → falls
/// back to Safe rendering automatically. See render_env.rs for the
/// full policy.
#[tauri::command]
fn mark_app_ready() {
    #[cfg(target_os = "linux")]
    {
        render_env::clear_recovery_marker(&config_dir());
    }
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
    // User-pasted secret supersedes any prior auto-registered session,
    // so clear the "you're on free tier" marker. Best-effort delete.
    let _ = std::fs::remove_file(auto_register_flag_file());
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

/// Marker file that exists exactly when the current secret on disk was
/// written by auto-register and the user has not yet acknowledged that
/// they're on a free-tier session. The frontend reads this flag to
/// decide whether to surface the "you're on free tier, want to upgrade?"
/// dialog. Cleared whenever the user either pastes a subscriber secret
/// (set_secret) or explicitly dismisses the prompt (acknowledge_free_tier).
fn auto_register_flag_file() -> PathBuf {
    config_dir().join("auto-registered")
}

/// Look for a configured secret. Env var wins; falls back to the on-disk
/// config file. Returns an empty string if neither has a usable value —
/// the caller may then auto-register a fresh free-tier device.
fn read_secret() -> String {
    resolve_secret(
        std::env::var("MANGAPLUS_SECRET").ok().as_deref(),
        &secret_file(),
    )
}

/// Pure resolution: env value (if non-empty after trimming) wins, then
/// the file contents (if readable and non-empty after trimming), else
/// empty string. Split from `read_secret` so it's unit-testable without
/// touching real env or filesystem.
fn resolve_secret(env_val: Option<&str>, path: &std::path::Path) -> String {
    if let Some(s) = env_val {
        let s = s.trim();
        if !s.is_empty() {
            return s.to_string();
        }
    }
    if let Ok(s) = std::fs::read_to_string(path) {
        return s.trim().to_string();
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::{filter_to_language, lang_str_to_enum, merge_views, resolve_secret};
    use mangaplus_api::proto;
    use std::io::Write;

    fn title(id: u32, name: &str, language: i32) -> proto::Title {
        proto::Title {
            title_id: id,
            name: name.to_string(),
            author: String::new(),
            portrait_image_url: String::new(),
            language,
            ..Default::default()
        }
    }

    fn group(titles: Vec<proto::Title>) -> proto::AllTitlesGroup {
        proto::AllTitlesGroup { titles }
    }

    fn view(groups: Vec<proto::AllTitlesGroup>) -> proto::SearchView {
        proto::SearchView {
            all_titles_group: groups,
            contents: vec![],
        }
    }

    #[test]
    fn filter_to_language_keeps_only_requested_lang() {
        let input = vec![
            title(1, "Bleach (eng)", 0),
            title(2, "Bleach (esp)", 1),
            title(3, "One Piece (eng)", 0),
            title(4, "Naruto (por)", 3),
        ];
        let kept = filter_to_language(input, "eng");
        let names: Vec<_> = kept.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["Bleach (eng)", "One Piece (eng)"]);
    }

    #[test]
    fn filter_to_language_handles_empty_input() {
        let kept = filter_to_language(vec![], "eng");
        assert!(kept.is_empty());
    }

    #[test]
    fn filter_to_language_drops_everything_for_no_match() {
        // Spanish requested, but every title is English-tagged → empty.
        let input = vec![title(1, "x", 0), title(2, "y", 0)];
        let kept = filter_to_language(input, "esp");
        assert!(kept.is_empty());
    }

    #[test]
    fn merge_views_dedupes_across_buckets_by_title_id() {
        // Same titleId appearing in both serializing and completed —
        // not impossible if the API briefly mid-relists during a status
        // change. Dedupe preserves the first occurrence.
        let serializing = view(vec![group(vec![
            title(100, "first", 0),
            title(101, "second", 0),
        ])]);
        let completed = view(vec![group(vec![
            title(100, "first (dupe)", 0),
            title(102, "third", 0),
        ])]);
        let merged = merge_views([serializing, completed], "eng");
        let names: Vec<_> = merged.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["first", "second", "third"]);
    }

    #[test]
    fn merge_views_applies_language_filter() {
        // Mixed languages in a single bucket → only requested lang
        // survives.
        let v = view(vec![group(vec![
            title(1, "eng-1", 0),
            title(2, "esp-1", 1),
            title(3, "eng-2", 0),
        ])]);
        let merged = merge_views([v], "eng");
        assert_eq!(merged.len(), 2);
        assert!(merged.iter().all(|t| t.language == 0));
    }

    #[test]
    fn merge_views_empty_input_yields_empty_output() {
        let merged: Vec<proto::Title> = merge_views([], "eng");
        assert!(merged.is_empty());
        // Also an iterator of empty views → empty.
        let merged = merge_views([view(vec![])], "eng");
        assert!(merged.is_empty());
    }

    #[test]
    fn merge_views_preserves_iteration_order() {
        // Order matters for UI scroll-restore. Dedupe must keep the
        // first-seen variant, not the last.
        let v1 = view(vec![group(vec![title(1, "alpha", 0)])]);
        let v2 = view(vec![group(vec![title(2, "beta", 0)])]);
        let v3 = view(vec![group(vec![title(3, "gamma", 0)])]);
        let merged = merge_views([v1, v2, v3], "eng");
        let ids: Vec<_> = merged.iter().map(|t| t.title_id).collect();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn lang_str_to_enum_matches_typescript_table() {
        // Mirrors LANG_ENUM_TO_CODE in reader/desktop/src/lib/lang.ts.
        // If the official app ever ships a new language, both this
        // match and that map need to grow together — this test is the
        // canary.
        assert_eq!(lang_str_to_enum("eng"), 0);
        assert_eq!(lang_str_to_enum("esp"), 1);
        assert_eq!(lang_str_to_enum("fra"), 2);
        assert_eq!(lang_str_to_enum("por"), 3);
        assert_eq!(lang_str_to_enum("rus"), 4);
        assert_eq!(lang_str_to_enum("ind"), 5);
    }

    #[test]
    fn lang_str_to_enum_unknown_falls_back_to_english() {
        // Garbage in shouldn't drop the whole catalog — preferring
        // English over an empty result list.
        assert_eq!(lang_str_to_enum(""), 0);
        assert_eq!(lang_str_to_enum("xyz"), 0);
    }

    #[test]
    fn lang_str_to_enum_is_case_insensitive() {
        // Without case folding, "ESP" would have fallen through to the
        // English default (0) — quietly serving English titles to a
        // Spanish user. The match arms only know lowercase, so the
        // function lowercases first.
        assert_eq!(lang_str_to_enum("ENG"), 0);
        assert_eq!(lang_str_to_enum("ESP"), 1);
        assert_eq!(lang_str_to_enum("Eng"), 0);
        assert_eq!(lang_str_to_enum("Por"), 3);
    }

    #[test]
    fn env_wins_over_file() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp, "from-file").unwrap();
        let resolved = resolve_secret(Some("from-env"), tmp.path());
        assert_eq!(resolved, "from-env");
    }

    #[test]
    fn falls_back_to_file_when_env_absent() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "  from-file").unwrap();
        let resolved = resolve_secret(None, tmp.path());
        assert_eq!(resolved, "from-file");
    }

    #[test]
    fn falls_back_to_file_when_env_is_blank() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp, "from-file").unwrap();
        let resolved = resolve_secret(Some("   "), tmp.path());
        assert_eq!(resolved, "from-file");
    }

    #[test]
    fn returns_empty_when_nothing_set() {
        let path = std::path::Path::new("/nonexistent/path/that/does/not/exist");
        let resolved = resolve_secret(None, path);
        assert_eq!(resolved, "");
    }
}

/// Register a brand-new device against the official endpoint and persist
/// the returned `deviceSecret` to the config file so subsequent launches
/// reuse it. The endpoint requires only an MD5(android_id) and the
/// salted MD5 of that — see `register_new_device` in mangaplus-api for
/// the wire details. Returns the new secret, or empty string on failure
/// (in which case the frontend's setup dialog takes over).
///
/// A self-registered secret has free-tier access only. If the user has
/// a paid subscription, they can paste their phone-extracted secret via
/// `set_secret` and it overwrites this one.
fn auto_register_secret() -> String {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("[mangaplus-reader] auto-register: runtime build failed: {e}");
            return String::new();
        }
    };
    match rt.block_on(register_new_device()) {
        Ok(secret) => {
            let path = secret_file();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            // Mark the session as auto-registered + unacknowledged so the
            // frontend can prompt the user about upgrading to subscriber.
            // Best-effort write — if it fails the only consequence is the
            // prompt doesn't show, the secret still works.
            let _ = std::fs::write(auto_register_flag_file(), "");
            if let Err(e) = std::fs::write(&path, &secret) {
                eprintln!("[mangaplus-reader] auto-register: persisting secret failed: {e}");
            }
            eprintln!("[mangaplus-reader] auto-register: registered new free-tier device");
            secret
        }
        Err(e) => {
            eprintln!("[mangaplus-reader] auto-register failed: {e}");
            String::new()
        }
    }
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

/// Stale-while-revalidate read of the full title catalog.
///
/// The MANGA Plus `title_list/all_v3` endpoint is split into two
/// publication-status buckets ("serializing" = ongoing, "completed" =
/// finished). To get the full catalog we fetch both in parallel and
/// merge — de-duplicating by `title_id`. The merged `Vec<Title>` is
/// cached as JSON (not proto bytes) since the wire format is two
/// separate SearchView responses and we'd rather not invent a synthetic
/// merged proto just for the cache.
///
/// Behaviour:
///   - Disk cache fresh  → return cached titles, `source = "fresh"`.
///   - Disk cache stale  → return cached titles, `source = "stale"`,
///     AND spawn a background refresh task. When it completes it
///     overwrites the disk cache and emits `all_titles_refreshed`.
///   - No disk cache     → block on the two network fetches, write
///     disk, return `source = "network"`.
///
/// Errors only when the no-cache path fails. Once a cache exists,
/// network failures during revalidation are logged and swallowed —
/// the user still gets the stale-but-usable data.
#[tauri::command]
async fn get_all_titles_cached(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    lang: String,
    clang: String,
) -> Result<AllTitlesPayload, String> {
    let ttl_hours = all_titles_cache::effective_ttl_hours();
    let cache_dir = all_titles_cache::cache_dir();

    if let Some(payload) = serve_from_cache(&state, &app, &lang, &clang, &cache_dir, ttl_hours) {
        return Ok(payload);
    }

    // No cache (or evicted-due-to-corrupt) → fetch the two buckets and
    // persist whatever we get back.
    let client = clone_client(&state)?;
    let titles = fetch_and_merge(&client, &lang, &clang)
        .await
        .map_err(|e| e.to_string())?;
    persist_titles(&cache_dir, &lang, &clang, &titles);
    Ok(AllTitlesPayload {
        titles,
        source: "network".to_string(),
        fetched_at_secs: all_titles_cache::now_secs(),
    })
}

/// Try to serve from the on-disk cache. Returns `Some(payload)` when a
/// usable cached snapshot exists (filtered, with the right `source`
/// label), and kicks off a background refresh when the snapshot is
/// stale. Returns `None` when:
///   - no cache file exists, OR
///   - the cache decoded into an unexpected shape and we evicted it.
///
/// In both `None` cases, the caller falls through to a synchronous
/// network fetch. Extracted from [`get_all_titles_cached`] so that
/// function reads as a three-step pipeline: try-cache, fetch, persist.
fn serve_from_cache(
    state: &tauri::State<'_, AppState>,
    app: &tauri::AppHandle,
    lang: &str,
    clang: &str,
    cache_dir: &std::path::Path,
    ttl_hours: u64,
) -> Option<AllTitlesPayload> {
    let (bytes, meta, fresh) = all_titles_cache::read(cache_dir, lang, clang, ttl_hours)?;
    // `serde_json::from_slice` enforces a recursion depth of 128 by
    // default, which bounds stack use against a deeply-nested
    // adversarial cache file. We rely on the default rather than
    // building a `Deserializer` explicitly so this stays one line.
    let titles: Vec<proto::Title> = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            // Old/unknown shape (e.g. a v0.9.3 build wrote proto bytes
            // here instead of JSON). Evict the bad pair and signal a
            // cache miss to the caller; returning `Err` from the
            // command would leave the user stuck on the curated
            // catalog with no recovery short of `rm -rf …`.
            eprintln!(
                "[all-titles] cache decode failed for ({lang}, {clang}): {e}. \
                 Evicting and refetching."
            );
            let (bin_path, meta_path) = all_titles_cache::cache_paths(cache_dir, lang, clang);
            let _ = std::fs::remove_file(&bin_path);
            let _ = std::fs::remove_file(&meta_path);
            return None;
        }
    };
    // Apply the language filter even on the read path so caches
    // written by builds before the filter shipped (v0.9.3) don't
    // bleed dupes into the UI. New caches are pre-filtered in
    // fetch_and_merge — belt and suspenders.
    let titles = filter_to_language(titles, lang);
    let source = match fresh {
        all_titles_cache::Freshness::Fresh => "fresh",
        all_titles_cache::Freshness::Stale => "stale",
    };
    if fresh == all_titles_cache::Freshness::Stale {
        spawn_refresh(
            state,
            app,
            lang.to_string(),
            clang.to_string(),
            cache_dir.to_path_buf(),
        );
    }
    Some(AllTitlesPayload {
        titles,
        source: source.to_string(),
        fetched_at_secs: meta.fetched_at_secs,
    })
}

/// Payload returned by [`get_all_titles_cached`]. `source` lets the
/// frontend distinguish the SWR cases (fresh / stale / network) without
/// having to read disk timestamps itself.
#[derive(serde::Serialize)]
struct AllTitlesPayload {
    titles: Vec<proto::Title>,
    source: String,
    fetched_at_secs: u64,
}

/// Fetch both publication-status buckets in parallel and merge into a
/// single deduplicated title list. The official app shows two tabs —
/// we paste them together because the desktop search UI is unified.
///
/// Unlike `/title_list/search` (which honours the `lang` query param
/// and returns only matching-language titles), `/title_list/all_v3`
/// ignores `lang` and returns every language variant — Akane-banashi
/// shows up once per dub (eng/esp/por/ind). We filter on the
/// `Title.language` enum so the result matches what the user asked for.
///
/// Uses `tokio::join!` (NOT `try_join!`) so a 429 on one bucket
/// doesn't lose the other. The merge collects whatever succeeded;
/// only a both-failed result propagates the error. A partial success
/// returns the half we have plus a log line.
async fn fetch_and_merge(
    client: &Client,
    lang: &str,
    clang: &str,
) -> Result<Vec<proto::Title>, mangaplus_api::ApiError> {
    let (serializing, completed) = tokio::join!(
        client.get_all_titles_by_type("serializing", lang, clang),
        client.get_all_titles_by_type("completed", lang, clang),
    );
    let serializing = match serializing {
        Ok(v) => Some(v),
        Err(e) => {
            eprintln!("[all-titles] /all_v3?type=serializing failed: {e}");
            None
        }
    };
    let completed = match completed {
        Ok(v) => Some(v),
        Err(e) => {
            eprintln!("[all-titles] /all_v3?type=completed failed: {e}");
            None
        }
    };
    // Both buckets failed — surface an error. One bucket OK → partial.
    if serializing.is_none() && completed.is_none() {
        return Err(mangaplus_api::ApiError::Other(
            "both /all_v3 buckets failed; cached copy (if any) still served".into(),
        ));
    }
    Ok(merge_views(serializing.into_iter().chain(completed), lang))
}

/// Flatten + dedupe + filter-by-language. Extracted as a pure
/// function over an iterator of SearchView so it can be unit-tested
/// without spinning up a client or fixture HTTP server.
fn merge_views(
    views: impl IntoIterator<Item = proto::SearchView>,
    lang: &str,
) -> Vec<proto::Title> {
    let want_lang = lang_str_to_enum(lang);
    let mut seen = std::collections::HashSet::<u32>::new();
    let mut merged: Vec<proto::Title> = Vec::new();
    for view in views {
        for group in view.all_titles_group {
            for t in group.titles {
                warn_unknown_language_once(t.language);
                if t.language != want_lang {
                    continue;
                }
                if seen.insert(t.title_id) {
                    merged.push(t);
                }
            }
        }
    }
    merged
}

/// Emit a one-time stderr warning the first time `merge_views` sees a
/// `Title.language` enum value outside the table in `lang_str_to_enum`.
/// MANGA Plus periodically adds languages; without this canary a new
/// enum value would silently filter out matching titles from the user's
/// catalog. The dedup is process-wide so log noise stays bounded.
fn warn_unknown_language_once(lang: i32) {
    use std::sync::{Mutex, OnceLock};
    static SEEN_UNKNOWN: OnceLock<Mutex<std::collections::HashSet<i32>>> = OnceLock::new();
    // Known enum range mirrors lang_str_to_enum. 0..=5 inclusive.
    if (0..=5).contains(&lang) {
        return;
    }
    let mu = SEEN_UNKNOWN.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
    let mut g = match mu.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    if g.insert(lang) {
        eprintln!(
            "[all-titles] Title.language={lang} not in lang_str_to_enum table — \
             new MANGA Plus language? Titles with this enum are silently filtered out."
        );
    }
}

/// Map a 3-letter language tag to MANGA Plus's wire enum. The mapping
/// mirrors `reader/desktop/src/lib/lang.ts::LANG_ENUM_TO_CODE` —
/// changing either side without the other will silently misfilter.
/// Returned as i32 because that's how prost generates Title.language.
///
/// Case-folds the input so "ESP" / "Esp" / "esp" all resolve to
/// Spanish. The earlier strict-match version treated "ESP" as unknown
/// and silently fell through to the English fallback (enum 0) —
/// quietly serving English titles to a Spanish user.
fn lang_str_to_enum(lang: &str) -> i32 {
    match lang.to_ascii_lowercase().as_str() {
        "eng" => 0,
        "esp" => 1,
        "fra" => 2,
        "por" => 3,
        "rus" => 4,
        "ind" => 5,
        _ => 0, // unknown → behave like English; better than dropping everything
    }
}

/// Keep only titles in the requested language. Used at every
/// boundary where multi-language Title lists leak in — the fresh-
/// fetch merge and the cache read path — so the UI is never asked to
/// render dupes across language variants.
fn filter_to_language(titles: Vec<proto::Title>, lang: &str) -> Vec<proto::Title> {
    let want = lang_str_to_enum(lang);
    titles.into_iter().filter(|t| t.language == want).collect()
}

fn persist_titles(cache_dir: &std::path::Path, lang: &str, clang: &str, titles: &[proto::Title]) {
    match serde_json::to_vec(titles) {
        Ok(bytes) => {
            all_titles_cache::write(cache_dir, lang, clang, &bytes, titles.len() as u32);
        }
        Err(e) => eprintln!("[all-titles] serialize merged titles failed: {e}"),
    }
}

/// Background revalidation: fetch fresh both buckets, rewrite disk,
/// emit `all_titles_refreshed` so the frontend can swap state.
/// Wrapped in the per-locale `RefreshGuards` so concurrent stale
/// reads don't dogpile.
fn spawn_refresh(
    state: &tauri::State<'_, AppState>,
    app: &tauri::AppHandle,
    lang: String,
    clang: String,
    cache_dir: PathBuf,
) {
    if !state.refresh_guards.try_acquire(&lang, &clang) {
        // Another stale-read for this locale already spawned a refresh
        // and is in flight — joining would duplicate work and might
        // race on the cache write. The in-flight refresh will emit
        // `all_titles_refreshed` when it lands, and every listening
        // page picks it up. Log so the silent-skip is visible.
        eprintln!(
            "[all-titles] refresh skipped: ({lang}, {clang}) already in flight"
        );
        return;
    }
    let client = match clone_client(state) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[all-titles] clone_client failed during refresh spawn: {e}");
            state.refresh_guards.release(&lang, &clang);
            return;
        }
    };
    let guards = state.refresh_guards.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match fetch_and_merge(&client, &lang, &clang).await {
            Ok(titles) => {
                persist_titles(&cache_dir, &lang, &clang, &titles);
                let payload = AllTitlesRefreshedEvent {
                    lang: lang.clone(),
                    clang: clang.clone(),
                    title_count: titles.len() as u32,
                    fetched_at_secs: all_titles_cache::now_secs(),
                    titles,
                };
                if let Err(e) = app.emit("all_titles_refreshed", &payload) {
                    eprintln!("[all-titles] emit refresh event failed: {e}");
                }
            }
            Err(e) => {
                eprintln!("[all-titles] background refresh failed: {e} (cached copy still served)");
            }
        }
        guards.release(&lang, &clang);
    });
}

#[derive(serde::Serialize, Clone)]
struct AllTitlesRefreshedEvent {
    lang: String,
    clang: String,
    title_count: u32,
    fetched_at_secs: u64,
    titles: Vec<proto::Title>,
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
    // Linux-only: pick a WebKit render mode based on the actual
    // hardware + display server, instead of unconditionally forcing
    // CPU rendering on every user. Replaces the always-on
    // WEBKIT_DISABLE_COMPOSITING_MODE hammer.
    //
    // Order of precedence (highest first):
    //   1. MANGAPLUS_RENDER_MODE env var
    //   2. <config_dir>/render.conf `mode = ...`
    //   3. Crash recovery (last run left a marker behind) → Safe
    //   4. Auto detect from GPU vendor + WAYLAND_DISPLAY
    //
    // Whatever's decided gets written to <config_dir>/render-state.log
    // so users can `cat` it and see exactly what was applied. See
    // docs/troubleshooting.md for the full matrix.
    #[cfg(target_os = "linux")]
    {
        use render_env::{
            apply_mode, decide_mode, detect_display_server_from_env,
            detect_gpu_vendor_from_sysfs, is_recovery_needed,
            create_recovery_marker, resolve_user_override, write_state_log,
            ModeOverride,
        };
        let cfg_dir = config_dir();
        let env_override = std::env::var("MANGAPLUS_RENDER_MODE").ok();
        let user_override = resolve_user_override(env_override.as_deref(), &cfg_dir);
        let recovery = is_recovery_needed(&cfg_dir);
        let explicit = match user_override {
            Some(ModeOverride::Explicit(m)) => Some(m),
            // ModeOverride::Auto is the same as "no override" — fall
            // through to crash-recovery / auto-detect.
            _ => None,
        };
        let display = detect_display_server_from_env();
        let gpu = detect_gpu_vendor_from_sysfs();
        let (mode, reason) = decide_mode(explicit, recovery, display, gpu);
        eprintln!(
            "[mangaplus-reader] render mode: {} ({}; display={:?} gpu={:?}{}{})",
            mode.slug(),
            reason,
            display,
            gpu,
            if user_override.is_some() { " override=yes" } else { "" },
            if recovery { " recovery=yes" } else { "" },
        );
        // SAFETY: this is the binary's first user-code call after main;
        // nothing has spawned a thread yet, so the env table has no
        // concurrent reader.
        unsafe { apply_mode(mode) };
        // Touch the recovery marker. If the frontend signals app-ready
        // before exit, we clear it via the mark_app_ready command. If
        // we crash mid-render, it stays — the next launch sees it and
        // falls back to Safe mode automatically.
        create_recovery_marker(&cfg_dir);
        write_state_log(
            &cfg_dir,
            mode,
            reason,
            display,
            gpu,
            user_override.is_some(),
            recovery,
        );
    }

    let mut secret = read_secret();
    eprintln!(
        "[mangaplus-reader] image cache: {}",
        image_cache_dir().display()
    );
    if secret.is_empty() {
        eprintln!("[mangaplus-reader] no deviceSecret configured — attempting fresh registration");
        secret = auto_register_secret();
        if secret.is_empty() {
            eprintln!("[mangaplus-reader] auto-register did not produce a secret — setup dialog will show");
        }
    }
    let client = rebuild_client(&secret).expect("build api client");
    let state = AppState {
        client: std::sync::Mutex::new(client.clone()),
        refresh_guards: Arc::new(all_titles_cache::RefreshGuards::default()),
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
            is_auto_registered,
            acknowledge_free_tier,
            mark_app_ready,
            set_secret,
            get_profile,
            get_favorites,
            add_favorite,
            remove_favorite,
            search,
            get_all_titles_cached,
            get_title_detail,
            get_chapter_pages,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
