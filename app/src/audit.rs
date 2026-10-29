//! Server-generated request correlation; never accepts client supplied identifiers.
use std::cell::RefCell;
tokio::task_local! { pub(crate) static REQUEST_ID: String; }
thread_local! { static CURRENT: RefCell<Option<String>> = const { RefCell::new(None) }; }
pub(crate) fn request_id() -> Option<String> {
    CURRENT.with(|current| current.borrow().clone())
}
pub(crate) fn with_request<T>(id: Option<String>, operation: impl FnOnce() -> T) -> T {
    struct Restore(Option<String>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CURRENT.with(|current| *current.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(CURRENT.with(|current| current.replace(id)));
    operation()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_context_restores_after_nested_calls_and_panics() {
        assert_eq!(request_id(), None);
        with_request(Some("outer".into()), || {
            let _ =
                std::panic::catch_unwind(|| with_request(Some("inner".into()), || panic!("test")));
            assert_eq!(request_id().as_deref(), Some("outer"));
        });
        assert_eq!(request_id(), None);
    }
    #[tokio::test]
    async fn http_history_correlates_server_request_and_ignores_client_header() {
        use axum::{body::Body, http::Request};
        use std::sync::{Arc, Mutex};
        use tower::ServiceExt;
        let mut store = crate::core::Store::memory().unwrap();
        store.seed_dev().unwrap();
        let store = Arc::new(Mutex::new(store));
        let app = crate::web::router(crate::web::AppState::new(
            store.clone(),
            "audit-test-token-long-enough".into(),
            "http://127.0.0.1:8092".into(),
        ));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/capture")
                    .header("authorization", "Bearer audit-test-token-long-enough")
                    .header("content-type", "application/json")
                    .header("idempotency-key", "audit-capture")
                    .header("x-request-id", "untrusted-client-id")
                    .body(Body::from(
                        r#"{"list_id":"inbox","tasks":[{"title":"Audit task"}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let request_id = response.headers()["x-request-id"].to_str().unwrap();
        assert_ne!(request_id, "untrusted-client-id");
        uuid::Uuid::parse_str(request_id).unwrap();
        let value: String = store
            .lock()
            .unwrap()
            .conn
            .query_row(
                "SELECT json_extract(event,'$.request_id') FROM history ORDER BY seq DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(value, request_id);
        assert_eq!(super::request_id(), None);
    }
}
