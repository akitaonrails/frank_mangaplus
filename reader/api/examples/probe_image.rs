//! Try to fetch a manga page image via reqwest (rustls) to see if it
//! gets the same 400 as curl, or if reqwest's TLS profile is accepted.

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let url = std::env::args().nth(1).expect("usage: probe_image <url>");
    let client = reqwest::Client::builder()
        .user_agent("okhttp/4.12.0")
        .build()
        .unwrap();
    let resp = client.get(&url).send().await.unwrap();
    println!("status: {}", resp.status());
    println!("content-type: {:?}", resp.headers().get("content-type"));
    let bytes = resp.bytes().await.unwrap();
    println!("body len: {}", bytes.len());
    if bytes.len() < 500 {
        println!("body text: {:?}", String::from_utf8_lossy(&bytes));
    }
}
