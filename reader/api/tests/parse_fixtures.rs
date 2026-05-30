//! Fixture-based tests for the protobuf decoder.
//!
//! Each fixture under `tests/fixtures/` is a raw protobuf body captured
//! from a real API call. Tests assert that we can decode them and pull
//! out the expected typed payload. No network access — these run in
//! `cargo test` and are deterministic.
//!
//! To capture a new fixture:
//!   curl -sS -o reader/api/tests/fixtures/<name>.bin \
//!     "https://jumpg-api.tokyo-cdn.com/api/<path>?os=android&os_ver=33&app_ver=250&secret=$MANGAPLUS_SECRET&..."

use mangaplus_api::proto::{self, response, success_result};
use prost::Message;

/// Decode a Response and extract `success.data`, panicking on error
/// variants — fine for tests since fixtures are known-good.
fn extract_data(bytes: &[u8]) -> success_result::Data {
    let resp = proto::Response::decode(bytes).expect("decode Response");
    match resp.result.expect("result oneof set") {
        response::Result::Success(s) => s.data.expect("success.data set"),
        response::Result::Error(e) => panic!(
            "fixture returned ErrorResult: action={} debug_info={:?}",
            e.action, e.debug_info
        ),
    }
}

const SEARCH_ENG: &[u8] = include_bytes!("fixtures/search_eng.bin");
const PROFILE: &[u8] = include_bytes!("fixtures/profile.bin");
const ERROR_INVALID_PARAMETER: &[u8] = include_bytes!("fixtures/error_invalid_parameter.bin");
const FAVORITES_4: &[u8] = include_bytes!("fixtures/favorites_4_titles.bin");
const TITLE_DETAIL_OP: &[u8] = include_bytes!("fixtures/title_detail_one_piece.bin");
const MANGA_VIEWER_OP_CH1: &[u8] = include_bytes!("fixtures/manga_viewer_op_ch1.bin");

// ---------- /title_list/bookmark ----------
//
// Guards against the original bug where the proto had favorite_titles_view
// at field 43 but the server actually returns SubscribedTitlesView at
// field 7. If someone "fixes" the field number back to 43, this test
// fails — making the regression loud.

#[test]
fn bookmark_returns_subscribed_titles_view() {
    let data = extract_data(FAVORITES_4);
    let view = match data {
        success_result::Data::SubscribedTitlesView(v) => v,
        other => panic!(
            "expected SubscribedTitlesView (field 7), got variant {:?}",
            std::mem::discriminant(&other)
        ),
    };
    assert!(!view.titles.is_empty(), "fixture has 4 titles, got none");
    // Sanity: every title has a non-zero titleId (catches off-by-one
    // field number errors in Title's transcription).
    for t in &view.titles {
        assert!(t.title_id > 0, "title with zero title_id: {:?}", t.name);
        assert!(!t.name.is_empty(), "title with empty name");
    }
}

// ---------- /title_detailV3 ----------

#[test]
fn title_detail_one_piece_has_chapter_list_v2() {
    let data = extract_data(TITLE_DETAIL_OP);
    let view = match data {
        success_result::Data::TitleDetailView(v) => v,
        _ => panic!("expected TitleDetailView (field 8)"),
    };
    assert!(view.title.is_some(), "title field 1 must be set");
    assert_eq!(view.title.as_ref().unwrap().title_id, 100020, "One Piece title_id mismatch");
    assert!(
        !view.chapter_list_v2.is_empty(),
        "expected chapter_list_v2 (field 38) populated for One Piece"
    );
    assert!(!view.overview.is_empty(), "overview should be populated");

    // Chapters should have non-zero IDs and names (guards Chapter field numbers).
    let first = &view.chapter_list_v2[0];
    assert!(first.chapter_id > 0, "chapter_id zero: {:?}", first.name);
    assert!(!first.name.is_empty(), "chapter name empty");
    assert_eq!(first.title_id, 100020, "Chapter.title_id should match parent");
}

// ---------- /manga_viewer_v3 ----------
//
// This is the test that would have caught the worst bug we hit: title_id
// at field 11 (wrong — was actually 9) and title_language as int32
// (wrong — was actually a string). Both would have panicked at decode
// time, but in CI not at user runtime.

#[test]
fn manga_viewer_one_piece_ch1_decodes() {
    let data = extract_data(MANGA_VIEWER_OP_CH1);
    let view = match data {
        success_result::Data::MangaViewer(v) => v,
        _ => panic!("expected MangaViewer (field 10)"),
    };

    // Specific fields that bit us before
    assert_eq!(view.chapter_id, 1000486, "chapter_id field 2 wrong");
    assert_eq!(view.title_id, 100020, "title_id MUST be at field 9 (was wrong at 11)");
    assert_eq!(view.title_language, "eng", "title_language MUST be string at field 15");
    assert_eq!(view.title_name, "One Piece");

    // Real pages
    assert!(view.pages.len() > 10, "expected many pages, got {}", view.pages.len());
    let manga_pages: Vec<_> = view
        .pages
        .iter()
        .filter_map(|p| p.data.as_ref().map(|d| match d {
            proto::page::Data::MangaPage(m) => m,
        }))
        .collect();
    assert!(!manga_pages.is_empty(), "expected at least some MangaPages");
    let first = manga_pages[0];
    assert!(first.image_url.starts_with("https://"), "image_url should be a full URL");
    assert!(first.image_url.contains("tokyo-cdn.com"), "expected tokyo-cdn host");
    assert!(first.width > 0 && first.height > 0, "expected width/height > 0");
}

#[test]
fn profile_decodes_to_profile_view() {
    // The fixture is ~40 KB but our sparse proto only exposes `user_name`;
    // the rest (icon_list, my_icon) gets discarded by prost as unknown fields.
    // user_name may be empty if the user hasn't set a display name in the
    // official app — what we're verifying here is just that the wire bytes
    // route through Response → SuccessResult → ProfileSettingsView correctly.
    let data = extract_data(PROFILE);
    match data {
        success_result::Data::ProfileSettingsView(_) => { /* expected */ }
        _ => panic!("expected ProfileSettingsView"),
    }
}

#[test]
fn error_response_decodes_to_error_variant() {
    use proto::response;
    let resp = proto::Response::decode(ERROR_INVALID_PARAMETER).expect("decode");
    match resp.result {
        Some(response::Result::Error(_e)) => { /* expected */ }
        Some(response::Result::Success(_)) => panic!("expected Error variant"),
        None => panic!("missing result oneof"),
    }
}

#[test]
fn search_eng_decodes() {
    let data = extract_data(SEARCH_ENG);
    let view = match data {
        success_result::Data::SearchView(v) => v,
        other => panic!("expected SearchView, got {:?}", std::mem::discriminant(&other)),
    };

    // English search catalog should have at least one "contents" section.
    assert!(!view.contents.is_empty(), "expected non-empty contents");
}

#[test]
fn search_eng_has_known_titles() {
    let data = extract_data(SEARCH_ENG);
    let view = match data {
        success_result::Data::SearchView(v) => v,
        _ => unreachable!(),
    };

    // Flatten every title across every section.
    let titles: Vec<&proto::Title> = view
        .contents
        .iter()
        .filter_map(|c| c.title_list.as_ref())
        .flat_map(|tl| tl.featured_titles.iter())
        .collect();

    assert!(!titles.is_empty(), "expected at least one title in some content section");

    // Sanity-check one well-known series. The English catalog should
    // always include One Piece somewhere.
    let names: Vec<&str> = titles.iter().map(|t| t.name.as_str()).collect();
    assert!(
        names.iter().any(|n| n.to_lowercase().contains("one piece")),
        "expected One Piece in search results; got first 10 names = {:?}",
        names.iter().take(10).collect::<Vec<_>>()
    );

    // Every title should have a non-zero title_id (otherwise the field
    // numbers are wrong).
    for t in &titles {
        assert!(t.title_id > 0, "title_id should be > 0 for {:?}", t.name);
    }
}

#[test]
fn search_eng_has_image_urls() {
    let data = extract_data(SEARCH_ENG);
    let view = match data {
        success_result::Data::SearchView(v) => v,
        _ => unreachable!(),
    };

    let with_image: usize = view
        .contents
        .iter()
        .filter_map(|c| c.title_list.as_ref())
        .flat_map(|tl| tl.featured_titles.iter())
        .filter(|t| !t.portrait_image_url.is_empty())
        .count();
    assert!(with_image > 10, "expected most titles to have portrait_image_url, only {with_image} did");
}
