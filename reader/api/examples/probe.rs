//! Live probe: call /title_list/bookmark via our reqwest+prost client
//! and print what we actually get back. Useful for diagnosing whether
//! the gzip / content-type handling is working end-to-end.
//!
//!   MANGAPLUS_SECRET=... cargo run --example probe

use mangaplus_api::{Client, ClientConfig};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let secret = std::env::var("MANGAPLUS_SECRET").expect("MANGAPLUS_SECRET");
    let client = Client::new(ClientConfig::new(secret)).unwrap();

    // Raw bytes first — see what the wire actually delivers.
    match client.get_raw("title_list/bookmark", &[]).await {
        Ok(body) => {
            println!("raw bookmark response: {} bytes", body.len());
            println!("first 32 bytes hex: {}", body.iter().take(32).map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" "));
            println!("strings:");
            for s in body.split(|&b| b == 0 || (b < 0x20 && b != b'\n')) {
                if s.len() > 8 {
                    if let Ok(t) = std::str::from_utf8(s) {
                        println!("  {:?}", t);
                    }
                }
            }
        }
        Err(e) => println!("get_raw error: {e}"),
    }

    println!("---");

    // And try the typed call.
    match client.get_favorites().await {
        Ok(v) => println!("get_favorites OK: {} titles", v.titles.len()),
        Err(e) => println!("get_favorites error: {e}"),
    }
}
