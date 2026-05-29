//! Probe /manga_viewer_v3 with a real chapter_id. Pulls the chapter list
//! from a title_detail call, picks the first chapter, fetches its pages,
//! and reports what oneof variant the server returned.

use mangaplus_api::{Client, ClientConfig};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let secret = std::env::var("MANGAPLUS_SECRET").expect("MANGAPLUS_SECRET");
    let client = Client::new(ClientConfig::new(secret)).unwrap();

    // 1. One Piece title detail to get a chapter_id we can use.
    let td = client
        .get_title_detail(100020, "eng", "eng", "US")
        .await
        .expect("get_title_detail");
    let chapters = if !td.chapter_list_v2.is_empty() {
        td.chapter_list_v2
    } else if let Some(group) = td.chapter_list_group {
        group.first_chapter_list
    } else {
        panic!("no chapters in title detail");
    };
    let chapter = chapters.first().expect("at least one chapter");
    println!("found chapter: id={} name={:?} sub={:?}", chapter.chapter_id, chapter.name, chapter.sub_title);

    // 2. Fetch its pages.
    match client
        .get_chapter_pages(chapter.chapter_id, "super_high", "horizontal", "eng", "US")
        .await
    {
        Ok(view) => {
            println!("MangaViewer OK: {} pages, titleName={:?}", view.pages.len(), view.title_name);
            let mp_count = view.pages.iter()
                .filter(|p| p.data.as_ref().and_then(|d| match d {
                    mangaplus_api::proto::page::Data::MangaPage(_) => Some(()),
                }).is_some())
                .count();
            println!("  of which {} are MangaPages (with image URLs)", mp_count);
            if let Some(first) = view.pages.iter().find_map(|p| p.data.as_ref().and_then(|d| match d {
                mangaplus_api::proto::page::Data::MangaPage(m) => Some(m),
            })) {
                println!("  first image: {}x{}", first.width, first.height);
                println!("  FULL URL: {}", first.image_url);
                println!("  encryption_key: {:?}", first.encryption_key);
            }
        }
        Err(e) => println!("get_chapter_pages error: {e}"),
    }
}
