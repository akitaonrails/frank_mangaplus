fn main() {
    let protos = [
        "proto/mangaplus.proto",
    ];

    for p in &protos {
        println!("cargo:rerun-if-changed={p}");
    }

    let mut config = prost_build::Config::new();
    config.bytes(["."]);
    // Tauri IPC serializes via serde_json — give every generated message
    // and oneof a Serialize derive so they cross the bridge transparently.
    config.type_attribute(".", "#[derive(serde::Serialize)]");
    config.type_attribute(".", "#[serde(rename_all = \"camelCase\")]");
    config
        .compile_protos(&protos, &["proto/"])
        .expect("protoc compile failed");
}
