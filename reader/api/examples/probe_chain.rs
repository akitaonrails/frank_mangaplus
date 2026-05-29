//! End-to-end: fetch chapter pages, then fetch one page image — both
//! through the same Client so the plus_vw_token cookie threads through.

use mangaplus_api::{Client, ClientConfig};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let secret = std::env::var("MANGAPLUS_SECRET").expect("MANGAPLUS_SECRET");
    let client = Client::new(ClientConfig::new(secret)).unwrap();

    let viewer = client
        .get_chapter_pages(1000486, "high", "horizontal", "eng", "US")
        .await
        .expect("get_chapter_pages");

    let first_url = viewer
        .pages
        .iter()
        .find_map(|p| p.data.as_ref().and_then(|d| match d {
            mangaplus_api::proto::page::Data::MangaPage(m) => Some(m.image_url.clone()),
        }))
        .expect("no manga page in viewer");

    println!("fetching: {first_url}");
    let (bytes, ct) = client.fetch_image(&first_url).await.expect("fetch_image");
    println!("OK: {} bytes  content-type: {ct}", bytes.len());
    println!("first 4 bytes (hex): {:02x} {:02x} {:02x} {:02x}", bytes[0], bytes[1], bytes[2], bytes[3]);
}
