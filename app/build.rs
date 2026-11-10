use sha2::{Digest, Sha256};
fn main() {
    println!("cargo:rerun-if-env-changed=TODOER_BUILD_VERSION");
    let version = std::env::var("TODOER_BUILD_VERSION")
        .unwrap_or_else(|_| std::env::var("CARGO_PKG_VERSION").expect("package version"));
    println!("cargo:rustc-env=TODOER_BUILD_VERSION={version}");
    println!("cargo:rerun-if-changed=assets");
    let mut digest = Sha256::new();
    digest.update(version.as_bytes());
    fn collect(directory: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(directory).expect("read frontend assets") {
            let path = entry.expect("read asset entry").path();
            if path.is_dir() {
                collect(&path, files);
            } else {
                files.push(path);
            }
        }
    }
    let mut files = Vec::new();
    collect(std::path::Path::new("assets"), &mut files);
    files.sort();
    for path in files {
        digest.update(path.as_os_str().as_encoded_bytes());
        digest.update(std::fs::read(&path).expect("read frontend asset"));
    }
    println!(
        "cargo:rustc-env=TODOER_ASSET_VERSION={:x}",
        digest.finalize()
    );
}
