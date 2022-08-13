fn main() {
    let mut app_version = String::from(env!("CARGO_PKG_VERSION"));
    app_version = format!("v{}",app_version);
    println!("cargo:rustc-env=APP_VERSION={}", app_version);

    tonic_build::configure()
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .compile(&["proto/server_status.proto"], &["proto"])
        .unwrap();
}