fn main() {
    configure_windows_openssl_linkage();
    tauri_build::build();
}

#[cfg(windows)]
fn configure_windows_openssl_linkage() {
    let openssl_dir = std::env::var("OPENSSL_DIR")
        .ok()
        .or_else(|| Some(r"C:\Program Files\OpenSSL-Win64".to_string()))
        .filter(|path| std::path::Path::new(path).exists());

    let Some(openssl_dir) = openssl_dir else {
        return;
    };

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let crt = if profile == "debug" { "MD" } else { "MD" };
    let lib_dir = format!(r"{openssl_dir}\lib\VC\x64\{crt}");

    if std::path::Path::new(&lib_dir).join("libcrypto.lib").exists() {
        println!("cargo:rustc-link-search=native={lib_dir}");
        println!("cargo:rustc-link-lib=dylib=libcrypto");
        println!("cargo:rustc-link-lib=dylib=libssl");
    }
}

#[cfg(not(windows))]
fn configure_windows_openssl_linkage() {}
