//! Wire-probe additional endpoints to see what oneof variants and other
//! fields the server actually returns at the top level of SuccessResult.
//! Throttled by the Client gate.

use mangaplus_api::{Client, ClientConfig};
use prost::Message;
use std::collections::BTreeMap;

fn read_varint(bytes: &[u8]) -> (u64, usize) {
    let mut val: u64 = 0;
    let mut shift = 0;
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        i += 1;
        val |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return (val, i);
        }
        shift += 7;
    }
    (val, i)
}

fn scan_field_tags(body: &[u8]) -> BTreeMap<u32, usize> {
    let mut field_counts = BTreeMap::new();
    let mut p = 0usize;
    while p < body.len() {
        let (tag, used) = read_varint(&body[p..]);
        p += used;
        let field = (tag >> 3) as u32;
        let wire = (tag & 0x7) as u32;
        *field_counts.entry(field).or_insert(0) += 1;
        match wire {
            0 => {
                let (_, u) = read_varint(&body[p..]);
                p += u;
            }
            1 => p += 8,
            2 => {
                let (len, u) = read_varint(&body[p..]);
                p += u;
                p += len as usize;
            }
            5 => p += 4,
            _ => {
                eprintln!("  unknown wire type {wire} at pos {p}, stopping scan");
                break;
            }
        }
    }
    field_counts
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let secret = std::env::var("MANGAPLUS_SECRET").expect("MANGAPLUS_SECRET");
    let client = Client::new(ClientConfig::new(secret)).unwrap();

    let probes: Vec<(&str, Vec<(&str, &str)>)> = vec![
        (
            "title_detailV3",
            vec![
                ("title_id", "100020"),
                ("lang", "eng"),
                ("clang", "eng"),
                ("country_code", "US"),
            ],
        ),
        (
            "home_v6",
            vec![
                ("lang", "eng"),
                ("viewer_mode", "horizontal"),
                ("clang", "eng"),
            ],
        ),
    ];

    for (path, query) in probes {
        println!("\n=== GET /{path}  query={query:?} ===");
        let body = match client.get_raw(path, &query).await {
            Ok(b) => b,
            Err(e) => {
                println!("  error: {e}");
                continue;
            }
        };
        println!("  raw response: {} bytes", body.len());

        // Decode outer envelope.
        let resp = match mangaplus_api::proto::Response::decode(&*body) {
            Ok(r) => r,
            Err(e) => {
                println!("  decode error: {e}");
                continue;
            }
        };

        use mangaplus_api::proto::response;
        match &resp.result {
            Some(response::Result::Error(e)) => {
                println!("  outer is Error: action={} debug={:?}", e.action, e.debug_info);
                continue;
            }
            Some(response::Result::Success(s)) => {
                println!("  outer is Success ({} bytes encoded)", s.encoded_len());
            }
            None => {
                println!("  outer empty");
                continue;
            }
        }

        // Pull just the Success payload bytes from the wire (skip the
        // Response field-1 tag + length prefix).
        let (outer_tag, mut p) = read_varint(&body);
        assert_eq!(outer_tag >> 3, 1, "expected Response field 1 = Success");
        let (success_len, used) = read_varint(&body[p..]);
        p += used;
        let success_body = &body[p..p + success_len as usize];

        let counts = scan_field_tags(success_body);
        println!("  Success body field tags found:");
        for (f, c) in &counts {
            println!("    field {f}: {c}x");
        }
    }
}
