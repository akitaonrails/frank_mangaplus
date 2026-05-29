use mangaplus_api::proto::{self, response, success_result};
use prost::Message;
fn main() {
    let bytes = std::fs::read("/tmp/fav.bin").unwrap();
    println!("body size: {}", bytes.len());
    let resp = proto::Response::decode(&*bytes).unwrap();
    match resp.result {
        Some(response::Result::Success(s)) => match s.data {
            Some(success_result::Data::FavoriteTitlesView(v)) => {
                println!("groups: {}", v.favorite_titles.len());
                for g in &v.favorite_titles {
                    println!("  lang={} titles={}", g.language, g.titles.len());
                    for t in g.titles.iter().take(5) {
                        println!("    {} - {}", t.title_id, t.name);
                    }
                }
            }
            other => println!("unexpected variant: {:?}", std::mem::discriminant(&other.unwrap())),
        },
        Some(response::Result::Error(e)) => println!("error: action={} debug={:?}", e.action, e.debug_info),
        None => println!("empty"),
    }
}
