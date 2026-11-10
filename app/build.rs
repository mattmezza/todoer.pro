use sha2::{Digest, Sha256};
fn main() {
    println!("cargo:rerun-if-env-changed=TODOER_BUILD_VERSION");
    let version = std::env::var("TODOER_BUILD_VERSION")
        .unwrap_or_else(|_| std::env::var("CARGO_PKG_VERSION").expect("package version"));
    println!("cargo:rustc-env=TODOER_BUILD_VERSION={version}");
    println!("cargo:rerun-if-changed=assets");
    let bundled_entries = std::fs::read_to_string("assets/bundles/entries.txt").unwrap_or_default();
    let script_prefix = if bundled_entries.is_empty() {
        ""
    } else {
        "bundles/"
    };
    println!("cargo:rustc-env=TODOER_SCRIPT_PREFIX={script_prefix}");
    println!("cargo:rustc-env=TODOER_BUNDLED_ENTRIES={bundled_entries}");
    for (filename, variable) in [
        ("icons.svg", "TODOER_ICONS_URL"),
        ("icon192-v2.png", "TODOER_ICON_URL"),
    ] {
        let bytes = std::fs::read(format!("assets/{filename}")).expect("read image asset");
        let hash = format!("{:x}", Sha256::digest(bytes));
        let (stem, extension) = filename.rsplit_once('.').expect("image extension");
        let url = format!("/assets/images/{stem}-{}.{extension}", &hash[..16]);
        println!("cargo:rustc-env={variable}={url}");
    }
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
