use std::{
    env,
    sync::{Arc, Mutex},
};
use todoer::{
    core::Store,
    web::{AppState, router},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::var("TODOER_DEV_AUTH").as_deref() != Ok("1") {
        return Err("Google authentication is not implemented yet. This increment runs only with TODOER_DEV_AUTH=1 on loopback; do not deploy.".into());
    }
    let path = env::var("TODOER_RUST_DB").unwrap_or_else(|_| ".local/rebuild/rust.db".into());
    if path == "deploy/data/todoer_local.db" || path == "deploy/data/todoer_prod.db" {
        return Err("Refusing legacy database path".into());
    }
    let token = env::var("TODOER_DEV_TOKEN")?;
    if token.len() < 24 {
        return Err("TODOER_DEV_TOKEN must contain at least 24 characters".into());
    }
    let mut store = Store::open(&path)?;
    store.seed_dev()?;
    let port: u16 = env::var("TODOER_RUST_PORT")
        .unwrap_or_else(|_| "8092".into())
        .parse()?;
    let state = AppState::new(
        Arc::new(Mutex::new(store)),
        token,
        format!("http://127.0.0.1:{port}"),
    );
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    eprintln!("Todoer development-only server: http://127.0.0.1:{port}");
    let shutdown_state = state.clone();
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async move {
            #[cfg(unix)]
            {
                let mut terminate =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("SIGTERM handler");
                tokio::select! {_ = tokio::signal::ctrl_c()=>{},_ = terminate.recv()=>{}}
            }
            #[cfg(not(unix))]
            {
                let _ = tokio::signal::ctrl_c().await;
            }
            shutdown_state.request_shutdown();
        })
        .await?;
    Ok(())
}
