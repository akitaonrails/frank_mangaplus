//! One-shot helper: read a captured SearchView fixture and print title_id
//! + name for entries matching a query string. Useful when we need a real
//! title_id without burning an extra live request.
//!
//!   cargo run -p mangaplus-api --example find_titles -- search_eng.bin "one piece"
//!
//! Will print up to 20 matches sorted by name.

use mangaplus_api::proto::{self, response, success_result};
use prost::Message;
use std::path::Path;

fn main() {
    let mut args = std::env::args().skip(1);
    let fixture = args
        .next()
        .expect("usage: find_titles <fixture.bin> <query>");
    let query = args.next().expect("usage: find_titles <fixture.bin> <query>");
    let query = query.to_lowercase();

    let path = Path::new("tests/fixtures").join(&fixture);
    let bytes = std::fs::read(&path).expect("read fixture");
    let resp = proto::Response::decode(&*bytes).expect("decode Response");
    let success = match resp.result {
        Some(response::Result::Success(s)) => s,
        Some(response::Result::Error(e)) => {
            panic!("fixture is an error: action={} debug={:?}", e.action, e.debug_info)
        }
        None => panic!("empty result oneof"),
    };
    let view = match success.data {
        Some(success_result::Data::SearchView(v)) => v,
        _ => panic!("expected SearchView in fixture"),
    };

    let mut matches: Vec<&proto::Title> = view
        .contents
        .iter()
        .filter_map(|c| c.title_list.as_ref())
        .flat_map(|tl| tl.featured_titles.iter())
        .filter(|t| t.name.to_lowercase().contains(&query))
        .collect();
    matches.sort_by_key(|t| t.name.clone());
    matches.dedup_by_key(|t| t.title_id);

    println!("matches for {query:?} in {fixture}:");
    for t in matches.iter().take(20) {
        println!("  {:>7}  lang={}  {:?}  by {:?}", t.title_id, t.language, t.name, t.author);
    }
    println!("total: {}", matches.len());
}
