fn main() {
    let mut app_version = String::from(env!("CARGO_PKG_VERSION"));
    app_version = format!("v{}", app_version);
    println!("cargo:rustc-env=APP_VERSION={}", app_version);
}