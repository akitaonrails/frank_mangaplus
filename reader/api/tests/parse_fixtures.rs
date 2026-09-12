//! Fixture-based tests for the protobuf decoder.
//!
//! Each fixture under `tests/fixtures/` is a raw protobuf body captured
//! from a real API call. Tests assert that we can decode them and pull
//! out the expected typed payload. No network access — these run in
//! `cargo test` and are deterministic.
//!
//! To capture a new fixture without exposing the device secret in shell
//! history, pass it to curl from its local config file:
//!   curl --silent --show-error --get \
//!     --output reader/api/tests/fixtures/<name>.bin \
//!     --data-urlencode "secret@/path/to/mangaplus-reader/secret" \
//!     --data-urlencode "os=android" --data-urlencode "os_ver=36" \
//!     --data-urlencode "app_ver=250" --data-urlencode "lang=eng" \
//!     --data-urlencode "clang=<code>" \
//!     "https://jumpg-api.tokyo-cdn.com/api/title_list/search"

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
const SEARCH_ESP: &[u8] = include_bytes!("fixtures/search_esp.bin");
const SEARCH_FRA: &[u8] = include_bytes!("fixtures/search_fra.bin");
const SEARCH_IND: &[u8] = include_bytes!("fixtures/search_ind.bin");
const SEARCH_PTB: &[u8] = include_bytes!("fixtures/search_ptb.bin");
const SEARCH_RUS: &[u8] = include_bytes!("fixtures/search_rus.bin");
const SEARCH_THA: &[u8] = include_bytes!("fixtures/search_tha.bin");
const SEARCH_VIE: &[u8] = include_bytes!("fixtures/search_vie.bin");
const SEARCH_DEU: &[u8] = include_bytes!("fixtures/search_deu.bin");
const PROFILE: &[u8] = include_bytes!("fixtures/profile.bin");
const ERROR_INVALID_PARAMETER: &[u8] = include_bytes!("fixtures/error_invalid_parameter.bin");
const ERROR_SUBSCRIPTION_LOCKED: &[u8] = include_bytes!("fixtures/error_subscription_locked.bin");
const SUBSCRIPTION_VIEW_DELUXE: &[u8] = include_bytes!("fixtures/subscription_view_deluxe.bin");
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
    let title = view.title.as_ref().unwrap();
    assert_eq!(title.title_id, 100020, "One Piece title_id mismatch");
    assert_eq!(
        title.language,
        mangaplus_api::lang::wire_enum(mangaplus_api::lang::ENGLISH),
        "title detail must identify the requested content language"
    );
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
fn subscription_locked_error_carries_english_popup() {
    // Captured from manga_viewer_v3 on a DELUXE-typed Bleach chapter
    // with a basic-plan account (2026-09-04). Guards the english_popup
    // (field 2) transcription — before it was declared, this refusal
    // surfaced as a bare "action=0" with no human-readable text.
    use proto::response;
    let resp = proto::Response::decode(ERROR_SUBSCRIPTION_LOCKED).expect("decode");
    let err = match resp.result {
        Some(response::Result::Error(e)) => e,
        _ => panic!("expected Error variant"),
    };
    let popup = err.english_popup.expect("english_popup decoded");
    assert_eq!(popup.subject, "Invalid user");
    assert_eq!(popup.body, "Invalid user access(11301)");
}

#[test]
fn subscription_view_carries_plan_and_payment_date() {
    // Captured from GET /api/subscription right after a deluxe restore
    // (2026-09-04). Guards the SuccessResult field-36 transcription and
    // UserSubscription's plan_type/next_payment_date field numbers —
    // the desktop's plan-drop warning depends on both.
    let data = extract_data(SUBSCRIPTION_VIEW_DELUXE);
    let view = match data {
        success_result::Data::SubscriptionView(v) => v,
        other => panic!(
            "expected SubscriptionView (field 36), got variant {:?}",
            std::mem::discriminant(&other)
        ),
    };
    let sub = view.user_subscription.expect("user_subscription set");
    assert_eq!(sub.plan_type, "deluxe");
    assert_eq!(sub.next_payment_date, 1_790_948_452);
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
fn translated_search_fixtures_confirm_content_language_wire_enums() {
    use mangaplus_api::lang;
    use std::collections::BTreeSet;

    // Each response was captured from /title_list/search with the matching
    // clang. This ties the production constants to language enum values
    // observed on the wire instead of relying only on synthetic Title values.
    let fixtures: [(&str, &[u8]); 9] = [
        (lang::ENGLISH, SEARCH_ENG),
        (lang::SPANISH, SEARCH_ESP),
        (lang::FRENCH, SEARCH_FRA),
        (lang::INDONESIAN, SEARCH_IND),
        (lang::PORTUGUESE_BR, SEARCH_PTB),
        (lang::RUSSIAN, SEARCH_RUS),
        (lang::THAI, SEARCH_THA),
        (lang::VIETNAMESE, SEARCH_VIE),
        (lang::GERMAN, SEARCH_DEU),
    ];

    for (clang, fixture) in fixtures {
        let data = extract_data(fixture);
        let view = match data {
            success_result::Data::SearchView(v) => v,
            other => panic!(
                "expected SearchView for clang={clang}, got {:?}",
                std::mem::discriminant(&other)
            ),
        };
        let titles: Vec<&proto::Title> = view
            .contents
            .iter()
            .filter_map(|content| content.title_list.as_ref())
            .flat_map(|list| list.featured_titles.iter())
            .collect();
        assert!(!titles.is_empty(), "empty search fixture for clang={clang}");

        let observed: BTreeSet<i32> = titles.iter().map(|title| title.language).collect();
        let expected = BTreeSet::from([lang::wire_enum(clang)]);
        assert_eq!(
            observed, expected,
            "search fixture contains unexpected language enums for clang={clang}"
        );
    }
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
