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
    let declined = Command::new(env!("CARGO_BIN_EXE_todoer"))
        .env("TODOER_URL", &url)
        .env("TODOER_TOKEN", TOKEN)
        .args(["move", &task_ref, "--to", &section_ref])
        .output()
        .unwrap();
    assert!(!declined.status.success());
    assert!(
        cli(&url, &["view", &section_ref])["children"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    cli(&url, &["move", &task_ref, "--to", &section_ref, "--yes"]);
    cli(&url, &["task-section", &task_ref, "--clear"]);
    cli(
        &url,
        &["task-section", &task_ref, "--section", &section_ref],
    );
    cli(&url, &["tags", "set", &task_ref, "--tag", "fresh"]);
    let comment = cli(
        &url,
        &[
            "comments",
            "add",
            &task_ref,
            "--key",
            "synthetic-comment",
            "Note",
        ],
    );
    let comment_id = comment["id"].as_str().unwrap();
    cli(&url, &["comments", "edit", comment_id, "Updated note"]);
    assert_eq!(
        cli(&url, &["comments", "get", comment_id])["body"],
        "Updated note"
    );
    let added = cli(
        &url,
        &[
            "task",
            "add",
            &format!("buy milk {list_ref} .tomo :1h +family !!"),
        ],
    );
    assert_eq!(added["task"]["title"], "buy milk");
    assert_eq!(added["capture"]["priority"], 1);
    assert_eq!(added["capture"]["duration"], 60);
    assert_eq!(added["capture"]["tags"], serde_json::json!(["family"]));
    let human = Command::new(env!("CARGO_BIN_EXE_todoer"))
        .env("TODOER_URL", &url)
        .env("TODOER_TOKEN", TOKEN)
        .args([
            "task",
            "add",
            &format!("buy bread {list_ref} .tomo :1h +family !!"),
        ])
        .output()
        .unwrap();
    assert!(
        human.status.success(),
        "{}",
        String::from_utf8_lossy(&human.stderr)
    );
    let human = String::from_utf8(human.stdout).unwrap();
    assert!(human.contains("Added P1 task 'buy bread'"));
    assert!(human.contains("(1h) +family"));
    assert!(human.contains("%buy bread"));
    let body = format!(
        r##"{{"list_id":"{list_ref}","title":"From JSON","metadata":{{"priority":1,"tags":["family"]}}}}"##
    );
    let from_json = cli(&url, &["task", "add", "--body", &body]);
    assert_eq!(from_json["task"]["title"], "From JSON");
    let found = cli(&url, &["query", "--list", &list_ref, "--text", "tomatoes"]);
    assert!(
        found["hits"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
    );
    let section = cli(&url, &["view", &section_ref]);
    assert_eq!(section["children"][0]["title"], "Ripe tomatoes");
    cli(&url, &["archive", &task_ref]);
    assert!(
        cli(&url, &["tasks", &list_ref, "--archived-only"])
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == task["entity"]["id"])
    );
    assert!(
        !cli(&url, &["trash-list"])
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == task["entity"]["id"])
    );
    cli(&url, &["restore", &task_ref]);
    cli(&url, &["archive", &section_ref]);
    assert!(
        cli(&url, &["sections", "all", &list_ref, "--archived-only"])
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == section["entity"]["id"])
    );
    assert!(
        cli(&url, &["tasks", &list_ref, "--archived"])
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == task["entity"]["id"] && row["archived"] == true)
    );
    cli(&url, &["restore", &section_ref]);
    cli(&url, &["trash", &task_ref]);
    cli(&url, &["restore", &task_ref]);
    cli(&url, &["archive", &list_ref]);
    assert!(
        !cli(&url, &["lists", "all"])
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == list["entity"]["id"])
    );
    assert!(
        cli(&url, &["lists", "all", "--archived-only"])
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == list["entity"]["id"] && row["archived"] == true)
    );
    cli(&url, &["restore", &list_ref]);
    cli(&url, &["archive", &space_ref]);
    assert!(
        cli(&url, &["spaces", "all", "--archived"])
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == space["entity"]["id"] && row["archived"] == true)
    );
    cli(&url, &["restore", &space_ref]);
    server.abort();
}
