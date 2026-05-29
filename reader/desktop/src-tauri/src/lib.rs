use mangaplus_api::{proto, Client, ClientConfig};
use std::sync::Arc;

/// Shared state held by Tauri and handed to every command invocation.
struct AppState {
    client: Arc<Client>,
}

/// Read `MANGAPLUS_SECRET` from the environment. Eventually we'll have
/// a settings UI; for now, fail fast with a clear message if missing.
fn read_secret() -> String {
    std::env::var("MANGAPLUS_SECRET").unwrap_or_else(|_| {
        eprintln!(
            "MANGAPLUS_SECRET not set. Extract one via the rooted AVD workflow \
             documented in docs/android-secret.md and export it before launching."
        );
        std::process::exit(2);
    })
}

// ---------- commands ----------
//
// Each typed command takes `tauri::State<AppState>` and returns
// `Result<T, String>` where T is a prost-generated proto type that has
// `#[derive(serde::Serialize)]` injected by api/build.rs.
//
// Tauri serializes the result through serde_json; the Svelte frontend
// receives camelCase JSON because of the type_attribute in build.rs.

#[tauri::command]
async fn get_profile(
    state: tauri::State<'_, AppState>,
) -> Result<proto::ProfileSettingsView, String> {
    state.client.get_profile().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_favorites(
    state: tauri::State<'_, AppState>,
) -> Result<proto::SubscribedTitlesView, String> {
    state.client.get_favorites().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn add_favorite(
    state: tauri::State<'_, AppState>,
    title_id: u32,
) -> Result<(), String> {
    state.client.add_favorite(title_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn remove_favorite(
    state: tauri::State<'_, AppState>,
    title_id: u32,
) -> Result<(), String> {
    state.client.remove_favorite(title_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn search(
    state: tauri::State<'_, AppState>,
    lang: String,
    clang: String,
) -> Result<proto::SearchView, String> {
    state.client.search(&lang, &clang).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_title_detail(
    state: tauri::State<'_, AppState>,
    title_id: u32,
    lang: String,
    clang: String,
    country_code: String,
) -> Result<proto::TitleDetailView, String> {
    state
        .client
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
    state
        .client
        .get_chapter_pages(chapter_id, &img_quality, &viewer_mode, &clang, &country_code)
        .await
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let secret = read_secret();
    let client = Client::new(ClientConfig::new(secret)).expect("build api client");
    let state = AppState { client: Arc::new(client) };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
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
