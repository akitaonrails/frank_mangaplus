use mangaplus_api::proto::{self, response};
use prost::Message;

fn main() {
    let bytes = std::fs::read("tests/fixtures/error_invalid_parameter.bin").unwrap();
    let resp = proto::Response::decode(&*bytes).unwrap();
    match resp.result {
        Some(response::Result::Error(e)) => {
            println!("action: {}", e.action);
            println!("debug_info: {:?}", e.debug_info);
        }
        Some(response::Result::Success(_)) => println!("unexpected: success"),
        None => println!("empty result"),
    }
}
