use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use todoer::core::{Capture, NewTask, Store};
use uuid::Uuid;

fn copy(operation: &str, source: &Path, target: &Path) -> Output {
    Command::new("python3")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/rebuild-backup.py"))
        .arg(operation)
        .arg(source)
        .arg(target)
        .output()
        .unwrap()
}

#[test]
fn online_backup_restores_synthetic_ids_and_refuses_overwrites() {
    let directory = std::env::temp_dir().join(format!("todoer-recovery-{}", Uuid::new_v4()));
    fs::create_dir(&directory).unwrap();
    let source = directory.join("source.sqlite3");
    let backup = directory.join("backup.sqlite3");
    let restored = directory.join("restored.sqlite3");
    let mut store = Store::open(source.to_str().unwrap()).unwrap();
    store.seed_dev().unwrap();
    let created = store
        .capture(
            "dev",
            "synthetic-recovery",
            Capture {
                list_id: "inbox".into(),
                tasks: vec![NewTask {
                    title: "Synthetic recovery fixture".into(),
                    description: "Generated test content only".into(),
                    parent_id: None,
                }],
            },
        )
        .unwrap();
    // Keep the writer open: this exercises committed WAL content, rather than
    // copying only the main file after a close/checkpoint.
    let before = fs::read(&source).unwrap();
    let result = copy("backup", &source, &backup);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    assert_eq!(fs::read(&source).unwrap(), before);
    assert_eq!(store.tasks("dev", "inbox").unwrap()[0].id, created[0].id);
    assert!(!copy("backup", &source, &backup).status.success());
    assert!(!copy("restore", &backup, &source).status.success());
    assert!(!copy("backup", &source, &source).status.success());
    assert!(
        !copy("backup", &source, &directory.join("missing/target.db"))
            .status
            .success()
    );
    assert!(copy("restore", &backup, &restored).status.success());
    let recovered = Store::open(restored.to_str().unwrap()).unwrap();
    assert_eq!(recovered.lists("dev").unwrap().len(), 2);
    let tasks = recovered.tasks("dev", "inbox").unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, created[0].id);
    assert_eq!(tasks[0].reference, created[0].reference);
    assert_eq!(tasks[0].description, created[0].description);
    assert_eq!(fs::read(&source).unwrap(), before);
    let legacy = directory.join("legacy.sqlite3");
    let legacy_db = rusqlite::Connection::open(&legacy).unwrap();
    legacy_db
        .execute_batch("CREATE TABLE users(uuid TEXT PRIMARY KEY); CREATE TABLE spaces(uuid TEXT);")
        .unwrap();
    drop(legacy_db);
    let rejected = directory.join("rejected.sqlite3");
    assert!(!copy("backup", &legacy, &rejected).status.success());
    assert!(!rejected.exists());
    let empty = directory.join("empty.sqlite3");
    fs::write(&empty, b"").unwrap();
    assert!(!copy("backup", &empty, &rejected).status.success());
    assert!(!rejected.exists());
    drop(recovered);
    drop(store);
    // The directory contains only files created by this test invocation.
    fs::remove_dir_all(directory).unwrap();
}
