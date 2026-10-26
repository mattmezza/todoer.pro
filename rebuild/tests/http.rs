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

#[tokio::test]
async fn search_filters_trash_and_restore_preserve_visibility() {
    let app = app();
    let token = bearer();
    let auth = [("authorization", token.as_str())];
    let (_, captured) = request(
        &app,
        "POST",
        "/api/v1/capture",
        json!({"list_id":"inbox","tasks":[{"title":"Proposal draft"},{"title":"Proposal review"}]}),
        &[("authorization", &token), ("idempotency-key", "search")],
    )
    .await;
    let id = captured[0]["id"].as_str().unwrap();
    let task_path = format!("/api/v1/tasks/{id}");
    let (status, completed) = request(
        &app,
        "PATCH",
        &task_path,
        json!({"revision":1,"complete":true}),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    for (complete, expected_id) in [(true, &captured[0]["id"]), (false, &captured[1]["id"])] {
        let (status, rows) = request(
            &app,
            "GET",
            &format!("/api/v1/search?query=prop&list=inbox&complete={complete}"),
            Value::Null,
            &auth,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert_eq!(&rows[0]["id"], expected_id);
    }
    let (status, rows) = request(
        &app,
        "GET",
        "/api/v1/search?query=proposal&list=groceries",
        Value::Null,
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(rows.as_array().unwrap().is_empty());
    let (status, error) = request(
        &app,
        "GET",
        "/api/v1/search?list=private",
        Value::Null,
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error["error"]["code"], "forbidden");
    let (status, trashed) = request(
        &app,
        "POST",
        &format!("{task_path}/trash"),
        json!({"revision":completed["revision"]}),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    for path in ["/api/v1/search?query=proposal", "/api/v1/lists/inbox/tasks"] {
        let (status, rows) = request(&app, "GET", path, Value::Null, &auth).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert_eq!(rows[0]["id"], captured[1]["id"]);
    }
    let (status, _) = request(
        &app,
        "GET",
        &format!("{task_path}/subtree"),
        Value::Null,
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, trash) = request(&app, "GET", "/api/v1/trash", Value::Null, &auth).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(trash[0]["id"], id);
    let (status, restored) = request(
        &app,
        "POST",
        &format!("{task_path}/restore"),
        json!({"revision":trashed["revision"]}),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(restored["id"], id);
    let (_, rows) = request(
        &app,
        "GET",
        "/api/v1/search?query=proposal&complete=true",
        Value::Null,
        &auth,
    )
    .await;
    assert_eq!(rows[0]["id"], id);
    let (_, trash) = request(&app, "GET", "/api/v1/trash", Value::Null, &auth).await;
    assert!(trash.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn undo_compensates_update_but_refuses_to_overwrite_newer_work() {
    let app = app();
    let token = bearer();
    let auth = [("authorization", token.as_str())];
    let (_, captured) = request(
        &app,
        "POST",
        "/api/v1/capture",
        json!({"list_id":"inbox","tasks":[{"title":"Original"}]}),
        &[("authorization", &token), ("idempotency-key", "undo")],
    )
    .await;
    let id = captured[0]["id"].as_str().unwrap();
    let path = format!("/api/v1/tasks/{id}");
    let (status, edited) = request(
        &app,
        "PATCH",
        &path,
        json!({"revision":1,"title":"Edited"}),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, history) = request(&app, "GET", "/api/v1/changes", Value::Null, &auth).await;
    let seq = history
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["kind"] == "update")
        .unwrap()["seq"]
        .as_i64()
        .unwrap();
    let undo_path = format!("/api/v1/history/{seq}/undo");
    let (status, undone) = request(
        &app,
        "POST",
        &undo_path,
        json!({"revision":edited["revision"]}),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(undone["title"], "Original");
    assert_eq!(undone["revision"].as_i64().unwrap(), 3);
    let (status, latest) = request(
        &app,
        "PATCH",
        &path,
        json!({"revision":3,"title":"Newer work"}),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, error) = request(
        &app,
        "POST",
        &undo_path,
        json!({"revision":latest["revision"]}),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["error"]["code"], "conflict");
    let (_, subtree) = request(&app, "GET", &format!("{path}/subtree"), Value::Null, &auth).await;
    assert_eq!(subtree[0]["title"], "Newer work");
}

#[tokio::test]
async fn logout_revokes_cookie_and_responses_have_security_headers() {
    let app = app();
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
    assert_eq!(response.headers()["cache-control"], "no-store");
    let csp = response.headers()["content-security-policy"]
        .to_str()
        .unwrap();
    assert!(csp.contains("default-src 'self'"));
    assert!(csp.contains("frame-ancestors 'none'"));
    assert!(csp.contains("form-action 'self'"));
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
    let (status, _) = request(
        &app,
        "GET",
        "/api/v1/lists",
        Value::Null,
        &[("cookie", &cookie)],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/logout")
                .header("cookie", &cookie)
                .header("x-csrf-token", csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(
        response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/lists")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert!(response.headers().contains_key("content-security-policy"));
}

#[tokio::test]
async fn mcp_rejects_missing_token_and_untrusted_origin_before_tool_dispatch() {
    let app = app();
    let body = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"capture","arguments":{"list_id":"inbox","idempotency_key":"must-not-run","tasks":[{"title":"Should never exist"}]}}});
    let (status, error) = request(&app, "POST", "/mcp", body.clone(), &[]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(error["error"]["code"], "unauthorized");
    let token = bearer();
    let (status, error) = request(
        &app,
        "POST",
        "/mcp",
        body,
        &[
            ("authorization", &token),
            ("origin", "https://untrusted.example"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error["error"]["code"], "origin");
    let (_, rows) = request(
        &app,
        "GET",
        "/api/v1/lists/inbox/tasks",
        Value::Null,
        &[("authorization", &token)],
    )
    .await;
    assert!(rows.as_array().unwrap().is_empty());
    let (_, history) = request(
        &app,
        "GET",
        "/api/v1/changes",
        Value::Null,
        &[("authorization", &token)],
    )
    .await;
    assert!(history.as_array().unwrap().is_empty());
}
