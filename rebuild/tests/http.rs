use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use todoer::{
    core::Store,
    web::{AppState, router},
};
use tower::ServiceExt;

const TOKEN: &str = "test-token-with-at-least-24-characters";
fn app() -> Router {
    let mut store = Store::memory().unwrap();
    store.seed_dev().unwrap();
    router(AppState::new(
        Arc::new(Mutex::new(store)),
        TOKEN.into(),
        "http://127.0.0.1:8092".into(),
    ))
}
async fn request(
    app: &Router,
    method: &str,
    path: &str,
    body: Value,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
fn bearer() -> String {
    format!("Bearer {TOKEN}")
}

#[tokio::test]
async fn auth_cookie_csrf_and_bearer_are_enforced() {
    let app = app();
    let capture = json!({"list_id":"inbox","tasks":[{"title":"Bread"}]});
    let (status, body) = request(&app, "POST", "/api/v1/capture", capture.clone(), &[]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "unauthorized");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/signin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let html = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    let csrf = html
        .split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/signin")
                .header("content-type", "application/x-www-form-urlencoded")
                .header("origin", "http://127.0.0.1:8092")
                .body(Body::from(format!("csrf={csrf}")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let (status, body) = request(
        &app,
        "POST",
        "/api/v1/capture",
        capture.clone(),
        &[("cookie", &cookie), ("idempotency-key", "cookie")],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "csrf");
    let (status, body) = request(
        &app,
        "POST",
        "/api/v1/capture",
        capture.clone(),
        &[
            ("cookie", &cookie),
            ("x-csrf-token", csrf),
            ("idempotency-key", "cookie"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body[0]["title"], "Bread");
    let (status, _) = request(
        &app,
        "POST",
        "/api/v1/capture",
        capture,
        &[("authorization", &bearer()), ("idempotency-key", "bearer")],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn capture_idempotency_revisions_and_private_access() {
    let app = app();
    let token = bearer();
    let headers = [
        ("authorization", token.as_str()),
        ("idempotency-key", "capture-1"),
    ];
    let payload = json!({"list_id":"inbox","tasks":[{"title":"Milk"}]});
    let (status, created) =
        request(&app, "POST", "/api/v1/capture", payload.clone(), &headers).await;
    assert_eq!(status, StatusCode::OK);
    let (status, retry) = request(&app, "POST", "/api/v1/capture", payload, &headers).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(retry, created);
    let (status, body) = request(
        &app,
        "POST",
        "/api/v1/capture",
        json!({"list_id":"inbox","tasks":[{"title":"Different"}]}),
        &headers,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "conflict");
    let path = format!("/api/v1/tasks/{}", created[0]["id"].as_str().unwrap());
    let (status, changed) = request(
        &app,
        "PATCH",
        &path,
        json!({"revision":created[0]["revision"],"title":"Oat milk"}),
        &headers,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(changed["title"], "Oat milk");
    let (status, body) = request(
        &app,
        "PATCH",
        &path,
        json!({"revision":created[0]["revision"],"complete":true}),
        &headers,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "conflict");
    let (status, lists) = request(&app, "GET", "/api/v1/lists", Value::Null, &headers).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        lists
            .as_array()
            .unwrap()
            .iter()
            .all(|list| list["id"] != "private")
    );
    let (status, body) = request(
        &app,
        "GET",
        "/api/v1/lists/private/tasks",
        Value::Null,
        &headers,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "forbidden");
    assert!(body.get("tasks").is_none());
    let (status, body) = request(
        &app,
        "POST",
        "/api/v1/capture",
        json!({"list_id":"private","tasks":[{"title":"Forbidden"}]}),
        &[("authorization", &token), ("idempotency-key", "private")],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "forbidden");
}

#[tokio::test]
async fn subtree_moves_children_with_parent_links_and_revisions() {
    let app = app();
    let token = bearer();
    let (status, root) = request(
        &app,
        "POST",
        "/api/v1/capture",
        json!({"list_id":"inbox","tasks":[{"title":"Dinner"}]}),
        &[("authorization", &token), ("idempotency-key", "root")],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let root = &root[0];
    let id = root["id"].as_str().unwrap();
    let (status, child) = request(
        &app,
        "POST",
        "/api/v1/capture",
        json!({"list_id":"inbox","tasks":[{"title":"Vegetables","parent_id":id}]}),
        &[("authorization", &token), ("idempotency-key", "child")],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let auth = [("authorization", token.as_str())];
    let path = format!("/api/v1/tasks/{id}/subtree");
    let (status, before) = request(&app, "GET", &path, Value::Null, &auth).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(before.as_array().unwrap().len(), 2);
    let (status, moved) = request(&app, "POST", &format!("/api/v1/tasks/{id}/move"), json!({"destination":"groceries","revision":root["revision"],"acknowledge_access_change":false}), &auth).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(moved["list_id"], "groceries");
    assert_eq!(
        moved["revision"].as_i64().unwrap(),
        root["revision"].as_i64().unwrap() + 1
    );
    let (status, after) = request(&app, "GET", &path, Value::Null, &auth).await;
    assert_eq!(status, StatusCode::OK);
    let rows = after.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|task| task["list_id"] == "groceries"));
    let moved_child = rows
        .iter()
        .find(|task| task["id"] == child[0]["id"])
        .unwrap();
    assert_eq!(moved_child["parent_id"], id);
    let (_, inbox) = request(&app, "GET", "/api/v1/lists/inbox/tasks", Value::Null, &auth).await;
    assert!(inbox.as_array().unwrap().is_empty());
}
