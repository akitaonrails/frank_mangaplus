//! Hits the live PUT /api/register endpoint with a freshly-generated
//! android_id, then proves the returned secret authenticates real reads
//! by calling /profile. Run with `cargo run --example register_smoketest`.

use mangaplus_api::{register_new_device, Client, ClientConfig};

#[tokio::main]
async fn main() {
    let secret = match register_new_device().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("register FAILED: {e}");
            std::process::exit(1);
        }
    };
    println!(
        "register OK: len={} hex_ok={} prefix={}…",
        secret.len(),
        secret.chars().all(|c| c.is_ascii_hexdigit()),
        &secret.chars().take(4).collect::<String>(),
    );

    let client = Client::new(ClientConfig::new(secret)).expect("client");
    match client.get_profile().await {
        Ok(p) => println!("profile OK: user_name=\"{}\"", p.user_name),
        Err(e) => eprintln!("profile FAILED: {e}"),
    }
}
