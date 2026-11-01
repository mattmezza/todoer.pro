use serde_json::Value;
use std::{
    process::Command,
    sync::{Arc, Mutex},
};
use todoer::{
    core::Store,
    web::{AppState, router},
};

const TOKEN: &str = "synthetic-cli-entity-workflow-token";

fn cli(url: &str, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_todoer"))
        .env("TODOER_URL", url)
        .env("TODOER_TOKEN", TOKEN)
        .arg("--json")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "command {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quick_edits_and_reviewed_moves_use_public_references() {
    let mut store = Store::memory().unwrap();
    store.seed_dev().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let app = router(AppState::new(
        Arc::new(Mutex::new(store)),
        TOKEN.into(),
        url.clone(),
    ));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let space = cli(
        &url,
        &["spaces", "create", "--title", "Synthetic CLI space"],
    );
    let space = cli(
        &url,
        &["view", &format!("@{}", space["id"].as_str().unwrap())],
    );
    let space_ref = format!("@{}", space["reference"].as_str().unwrap());
    let list = cli(
        &url,
        &[
            "lists",
            "create",
            "--space",
            &space_ref,
            "--title",
            "Synthetic list",
        ],
    );
    let list = cli(
        &url,
        &["view", &format!("#{}", list["id"].as_str().unwrap())],
    );
    let list_ref = format!("#{}", list["reference"].as_str().unwrap());
    let section = cli(
        &url,
        &["sections", "create", &list_ref, "--title", "Produce"],
    );
    let section = cli(
        &url,
        &["view", &format!("${}", section["id"].as_str().unwrap())],
    );
    let section_ref = format!("${}", section["reference"].as_str().unwrap());
    let task = cli(
        &url,
        &[
            "capture",
            "--list",
            &list_ref,
            "--key",
            "synthetic-cli-workflow",
            "Tomatoes",
        ],
    );
    let task = cli(
        &url,
        &["view", &format!("%{}", task[0]["id"].as_str().unwrap())],
    );
    let task_ref = format!("%{}", task["reference"].as_str().unwrap());
    cli(&url, &["edit", &task_ref, "--title", "Ripe tomatoes"]);
    cli(&url, &["move", &task_ref, "--to", &section_ref, "--yes"]);
    let section = cli(&url, &["view", &section_ref]);
    assert_eq!(section["children"][0]["title"], "Ripe tomatoes");
    server.abort();
}
