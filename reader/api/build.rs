fn main() {
    let protos = [
        "proto/mangaplus.proto",
    ];

    for p in &protos {
        println!("cargo:rerun-if-changed={p}");
    }

    let mut config = prost_build::Config::new();
    config.bytes(&["."]);
    config
        .compile_protos(&protos, &["proto/"])
        .expect("protoc compile failed");
}
