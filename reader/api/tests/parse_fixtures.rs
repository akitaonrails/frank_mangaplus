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
