use sha2::{Digest, Sha256};
fn main() {
    println!("cargo:rerun-if-changed=assets");
    let mut digest = Sha256::new();
    for name in [
        "app.js",
        "app.css",
        "offline.js",
        "pwa.js",
        "htmx.min.js",
        "manifest.webmanifest",
    ] {
        digest.update(name.as_bytes());
        if let Ok(bytes) = std::fs::read(format!("assets/{name}")) {
            digest.update(bytes);
        }
    }
    println!(
        "cargo:rustc-env=TODOER_ASSET_VERSION={:x}",
        digest.finalize()
    );
}
