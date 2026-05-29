//! Test image fetch via wreq with Android OkHttp impersonation. If this
//! returns 200 + image bytes, we've confirmed the production fix path:
//! route all image fetches through wreq.

use wreq_util::Emulation;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let url = std::env::args().nth(1).expect("usage: probe_image_wreq <url>");

    // Try a few presets — see which one the CDN accepts.
    for emu in [Emulation::OkHttp5, Emulation::OkHttp4_12, Emulation::OkHttp4_9, Emulation::OkHttp3_14] {
        println!("\n=== {:?} ===", emu);
        let client = match wreq::Client::builder().emulation(emu).build() {
            Ok(c) => c,
            Err(e) => { println!("build failed: {e}"); continue; }
        };
        match client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status();
                let body = resp.bytes().await.unwrap_or_default();
                println!("status: {} body_len: {}", status, body.len());
                if status.is_success() {
                    println!("SUCCESS — first 8 bytes: {:?}", &body[..body.len().min(8)]);
                    break;
                }
            }
            Err(e) => println!("request error: {e}"),
        }
    }
}
