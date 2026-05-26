use std::path::Path;

fn main() {
    link_unixodbc_on_macos();
    tauri_build::build()
}

fn link_unixodbc_on_macos() {
    if !cfg!(target_os = "macos") {
        return;
    }

    let candidates = [
        "/opt/homebrew/opt/unixodbc/lib",
        "/opt/homebrew/lib",
        "/usr/local/opt/unixodbc/lib",
        "/usr/local/lib",
    ];

    for candidate in candidates {
        if Path::new(candidate).exists() {
            println!("cargo:rustc-link-search=native={}", candidate);
        }
    }
}
