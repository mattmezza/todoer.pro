use std::{
    env,
    sync::{Arc, Mutex},
};
use todoer::{
    core::Store,
    web::{AppState, router},
};

fn initial_flag(name:&str,default:bool)->Result<bool,Box<dyn std::error::Error>> {
    match env::var(name) {Ok(v)=>match v.as_str(){"true"|"1"=>Ok(true),"false"|"0"=>Ok(false),_=>Err(format!("{name} must be true, false, 1 or 0").into())},Err(env::VarError::NotPresent)=>Ok(default),Err(e)=>Err(e.into())}
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let migrate_only = match arguments.as_slice() {
        [] => false,
        [flag] if flag == "--migrate-only" => true,
        _ => return Err("Usage: todoer-server [--migrate-only]".into()),
    };
    let development = env::var("TODOER_DEV_AUTH").as_deref() == Ok("1");
    let path = if development {
        env::var("TODOER_RUST_DB").unwrap_or_else(|_| ".local/rebuild/rust.db".into())
    } else {
        env::var("TODOER_RUST_DB")?
    };
    if path == "deploy/data/todoer_local.db" || path == "deploy/data/todoer_prod.db" {
        return Err("Refusing legacy database path".into());
    }
    let mut store = Store::open(&path)?;
    if migrate_only {
        eprintln!("Database migrations complete; no listener or notification workers started");
        return Ok(());
    }
    let google_enabled = initial_flag("TODOER_GOOGLE_LOGIN_ENABLED",true)?;
    let signup_enabled = initial_flag("TODOER_SIGNUP_ENABLED",true)?;
    store.initialize_auth_policy(google_enabled,signup_enabled)?;
    let port: u16 = env::var("TODOER_RUST_PORT")
        .unwrap_or_else(|_| "8092".into())
        .parse()?;
    let (state, bind) = if development {
        let token = env::var("TODOER_DEV_TOKEN")?;
        if token.len() < 24 {
            return Err("TODOER_DEV_TOKEN must contain at least24 characters".into());
        }
        store.seed_dev()?;
        (
            AppState::new(
                Arc::new(Mutex::new(store)),
                token,
                format!("http://127.0.0.1:{port}"),
            ),
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        )
    } else {
        let origin = env::var("TODOER_PUBLIC_URL")?
            .trim_end_matches('/')
            .to_string();
        let url = reqwest::Url::parse(&origin)?;
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
            || url.scheme() != "https"
        {
            return Err(
                "TODOER_PUBLIC_URL must be an HTTPS origin without path, query or credentials"
                    .into(),
            );
        }
        let mut auth = todoer::auth::AuthStore::open(&path)?;
        let allowlist = env::var("TODOER_REGISTRATION_ALLOWLIST")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        auth.replace_allowlist(&allowlist)?;
        let oidc = if env::var("TODOER_GOOGLE_CLIENT_ID").is_ok() || env::var("TODOER_GOOGLE_CLIENT_SECRET").is_ok() { Some(todoer::oidc::GoogleOidc::discover(todoer::oidc::GoogleConfig {
            client_id: env::var("TODOER_GOOGLE_CLIENT_ID")?,
            client_secret: env::var("TODOER_GOOGLE_CLIENT_SECRET")?,
            redirect_uri: format!("{origin}/auth/google/callback"),
        })
        .await?) } else { None };
        if auth.auth_policy()?.google_enabled && oidc.is_none() { return Err("Google login is enabled but client credentials are missing".into()); }
        let bind = env::var("TODOER_RUST_BIND")
            .unwrap_or_else(|_| "127.0.0.1".into())
            .parse::<std::net::IpAddr>()?;
        (
            AppState::production_optional(Arc::new(Mutex::new(store)), auth, oidc, origin),
            bind,
        )
    };
    let storage = todoer::object_storage::S3Config::from_env()?
        .map(todoer::object_storage::ObjectStorage::new)
        .transpose()?;
    let signer_key = env::var("TODOER_SERVER_KEY").or_else(|error| {
        if development {
            Ok(format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            ))
        } else {
            Err(error)
        }
    })?;
    let state = state.with_account_key(&signer_key).with_invitations(todoer::invitations::InvitationSigner::new(
        &signer_key,
        if development {
            format!("http://127.0.0.1:{port}")
        } else {
            env::var("TODOER_PUBLIC_URL")?
                .trim_end_matches('/')
                .to_owned()
        },
    )?);
    let state = state
        .with_assist(todoer::assist::AssistService::from_env()?)
        .with_storage(storage)
        .with_push(todoer::push::PushTransport::from_env()?);
    let statistics_worker = state.start_statistics_worker();
    let push_worker = state.start_push_worker();
    let notifications = todoer::notifications::Notifications::from_env()?;
    let worker = notifications.map(|config| state.start_notification_worker(config));
    let listener = tokio::net::TcpListener::bind((bind, port)).await?;
    eprintln!(
        "Todoer server listening on {} ({})",
        listener.local_addr()?,
        if development {
            "isolated development"
        } else {
            "Google authentication"
        }
    );
    let shutdown_state = state.clone();
    axum::serve(listener, router(state.clone()))
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
    state.request_shutdown();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(10), statistics_worker).await;
    if let Some(worker) = push_worker {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(30), worker).await;
    }
    if let Some(worker) = worker {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(30), worker).await;
    }
    Ok(())
}
