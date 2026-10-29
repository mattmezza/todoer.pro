use sha2::{Digest, Sha256};
fn main() {
    println!("cargo:rerun-if-env-changed=TODOER_BUILD_VERSION");
    let version = std::env::var("TODOER_BUILD_VERSION")
        .unwrap_or_else(|_| std::env::var("CARGO_PKG_VERSION").expect("package version"));
    println!("cargo:rustc-env=TODOER_BUILD_VERSION={version}");
    println!("cargo:rerun-if-changed=assets");
    let mut digest = Sha256::new();
    let mut files = std::fs::read_dir("assets")
        .expect("read frontend assets")
        .map(|entry| entry.expect("read asset entry").path())
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    files.sort();
    for path in files {
        digest.update(path.file_name().expect("asset filename").as_encoded_bytes());
        digest.update(std::fs::read(&path).expect("read frontend asset"));
    }
    println!(
        "cargo:rustc-env=TODOER_ASSET_VERSION={:x}",
        digest.finalize()
    );
}
