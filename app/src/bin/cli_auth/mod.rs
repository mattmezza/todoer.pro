use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Write, path::PathBuf, time::Duration};
use todoer::cli_auth::LoginGrant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
type Failure = (i32, String);
fn failure(message: impl Into<String>) -> Failure {
    (3, message.into())
}
fn origin(value: &str) -> Result<String, Failure> {
    let url = reqwest::Url::parse(value).map_err(|_| failure("Invalid Todoer URL"))?;
    let local = matches!(
        url.host_str(),
        Some("127.0.0.1" | "[::1]" | "::1" | "localhost")
    );
    if (url.scheme() != "https" && !(url.scheme() == "http" && local))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(failure(
            "Login requires an HTTPS origin, or HTTP localhost for development",
        ));
    }
    Ok(url.to_string().trim_end_matches('/').into())
}
fn path() -> Result<PathBuf, Failure> {
    let root = if let Some(root) = std::env::var_os("TODOER_CONFIG_DIR") {
        PathBuf::from(root)
    } else if let Some(root) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(root).join("todoer")
    } else if cfg!(windows) {
        PathBuf::from(
            std::env::var_os("APPDATA")
                .ok_or_else(|| failure("Set TODOER_CONFIG_DIR to store CLI credentials"))?,
        )
        .join("todoer")
    } else {
        PathBuf::from(
            std::env::var_os("HOME")
                .ok_or_else(|| failure("Set TODOER_CONFIG_DIR to store CLI credentials"))?,
        )
        .join(".config/todoer")
    };
    Ok(root.join("credentials.json"))
}
fn read() -> Result<BTreeMap<String, LoginGrant>, Failure> {
    let path = path()?;
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(_) => return Err(failure("Could not inspect CLI credentials")),
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(failure("CLI credential file must not be a symlink"));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if meta.permissions().mode() & 0o077 != 0 {
                    return Err(failure(
                        "CLI credentials must be private: chmod 600 the credentials.json file",
                    ));
                }
            }
        }
    }
    serde_json::from_slice(&fs::read(path).map_err(|_| failure("Could not read CLI credentials"))?)
        .map_err(|_| failure("Invalid CLI credential file"))
}
fn write(values: &BTreeMap<String, LoginGrant>) -> Result<(), Failure> {
    let target = path()?;
    let parent = target.parent().unwrap();
    fs::create_dir_all(parent).map_err(|_| failure("Could not create CLI config directory"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .map_err(|_| failure("Could not secure CLI config directory"))?;
    }
    let temp = parent.join(format!(".credentials-{}.tmp", uuid::Uuid::new_v4()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options
            .open(&temp)
            .map_err(|_| failure("Could not create private CLI credentials"))?;
        file.write_all(
            &serde_json::to_vec_pretty(values)
                .map_err(|_| failure("Could not encode credentials"))?,
        )
        .map_err(|_| failure("Could not write CLI credentials"))?;
        file.sync_all()
            .map_err(|_| failure("Could not save CLI credentials"))?;
        fs::rename(&temp, &target).map_err(|_| failure("Could not replace CLI credentials"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
pub fn token(url: &str) -> Result<String, Failure> {
    if let Ok(value) = std::env::var("TODOER_TOKEN")
        && !value.trim().is_empty()
    {
        return Ok(value);
    }
    let key = origin(url)?;
    let mut values = read()?;
    let grant = values
        .remove(&key)
        .ok_or_else(|| failure("Not logged in. Run todoer login, or set TODOER_TOKEN."))?;
    if grant.expires_at <= chrono::Utc::now().timestamp() {
        return Err(failure("CLI login expired. Run todoer login again."));
    }
    Ok(grant.token)
}
fn client() -> Result<Client, Failure> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| failure("Could not initialize HTTP client"))
}
fn browser(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = std::process::Command::new("xdg-open");
    command
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}
async fn callback(listener: tokio::net::TcpListener, state: &str) -> Result<String, Failure> {
    loop {
        let (mut stream, _) = listener
            .accept()
            .await
            .map_err(|_| failure("Could not receive browser callback"))?;
        let mut request = Vec::new();
        let read = tokio::time::timeout(Duration::from_secs(3), async {
            while request.len() < 8192 && !request.windows(4).any(|s| s == b"\r\n\r\n") {
                let mut bytes = [0u8; 1024];
                let n = stream.read(&mut bytes).await?;
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&bytes[..n]);
            }
            Ok::<_, std::io::Error>(())
        })
        .await;
        if !matches!(read, Ok(Ok(()))) {
            continue;
        }
        let request = String::from_utf8_lossy(&request);
        let Some(target) = request
            .lines()
            .next()
            .and_then(|s| s.strip_prefix("GET "))
            .and_then(|s| s.split_whitespace().next())
        else {
            continue;
        };
        let Ok(url) = reqwest::Url::parse(&format!("http://127.0.0.1{target}")) else {
            continue;
        };
        let query = url.query_pairs().collect::<BTreeMap<_, _>>();
        let valid = url.path() == "/callback" && query.get("state").is_some_and(|s| s == state);
        let text = if valid {
            "Todoer CLI received your response. You can close this tab and return to the terminal."
        } else {
            "Invalid login callback."
        };
        let response = format!(
            "HTTP/1.1 {}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            if valid { "200 OK" } else { "400 Bad Request" },
            text.len(),
            text
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
        if !valid {
            continue;
        }
        if query.contains_key("error") {
            return Err(failure("Login was denied in the browser"));
        }
        if let Some(code) = query.get("code") {
            return Ok(code.to_string());
        }
    }
}
pub async fn login(
    url: &str,
    permission: &str,
    no_browser: bool,
    timeout: u64,
) -> Result<serde_json::Value, Failure> {
    let base = origin(url)?;
    if !(10..=600).contains(&timeout) {
        return Err(failure("Login timeout must be 10–600 seconds"));
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| failure("Could not listen on loopback for browser login"))?;
    let redirect_uri = format!(
        "http://127.0.0.1:{}/callback",
        listener
            .local_addr()
            .map_err(|_| failure("Could not determine callback port"))?
            .port()
    );
    let verifier = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let state = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let client = client()?;
    let response=client.post(format!("{base}/api/cli/login/start")).json(&serde_json::json!({"redirect_uri":redirect_uri,"challenge":challenge,"state":state,"permission":permission})).send().await.map_err(|_|failure("Could not start CLI login"))?;
    if !response.status().is_success() {
        return Err(failure(format!(
            "Server refused CLI login ({})",
            response.status()
        )));
    }
    let start: serde_json::Value = response
        .json()
        .await
        .map_err(|_| failure("Invalid login response"))?;
    let authorization = start["authorization_url"]
        .as_str()
        .ok_or_else(|| failure("Missing browser authorization URL"))?;
    let request_id = start["request_id"]
        .as_str()
        .ok_or_else(|| failure("Missing login request ID"))?;
    if !authorization.starts_with(&format!("{base}/cli/authorize?")) {
        return Err(failure("Server returned an unexpected authorization URL"));
    }
    eprintln!("Approve CLI access in your browser:\n{authorization}");
    if !no_browser && !browser(authorization) {
        eprintln!("Could not open a browser automatically; open the URL above.");
    }
    let code = tokio::time::timeout(Duration::from_secs(timeout), callback(listener, &state))
        .await
        .map_err(|_| failure("Login timed out. Run todoer login again."))??;
    let response = client
        .post(format!("{base}/api/cli/login/token"))
        .json(&serde_json::json!({"request_id":request_id,"code":code,"verifier":verifier}))
        .send()
        .await
        .map_err(|_| failure("Could not exchange CLI authorization"))?;
    if !response.status().is_success() {
        return Err(failure(format!(
            "Login exchange failed ({})",
            response.status()
        )));
    }
    let grant: LoginGrant = response
        .json()
        .await
        .map_err(|_| failure("Invalid CLI credential response"))?;
    let metadata = serde_json::json!({"server":base,"credential_id":grant.credential_id,"expires_at":grant.expires_at,"permission":grant.permission,"spaces":grant.spaces,"lists":grant.lists,"all_resources":grant.all_resources});
    let mut values = read()?;
    values.insert(base.clone(), grant);
    write(&values)?;
    Ok(metadata)
}
pub async fn logout(url: &str) -> Result<serde_json::Value, Failure> {
    let base = origin(url)?;
    let mut values = read()?;
    if let Some(grant) = values.get(&base) {
        let response = client()?
            .post(format!("{base}/api/cli/login/revoke"))
            .bearer_auth(&grant.token)
            .send()
            .await
            .map_err(|_| {
                failure("Could not revoke login. Credentials kept so you can retry logout.")
            })?;
        if !response.status().is_success() {
            return Err(failure(
                "Server did not confirm revocation. Credentials kept so you can retry.",
            ));
        }
        values.remove(&base);
        write(&values)?;
    }
    Ok(
        serde_json::json!({"server":base,"logged_in":false,"environment_token_set":std::env::var("TODOER_TOKEN").is_ok()}),
    )
}
pub fn status(url: &str) -> Result<serde_json::Value, Failure> {
    let base = origin(url)?;
    if std::env::var("TODOER_TOKEN").is_ok_and(|v| !v.trim().is_empty()) {
        return Ok(
            serde_json::json!({"server":base,"source":"TODOER_TOKEN","saved_login_present":read()?.contains_key(&base)}),
        );
    }
    let values = read()?;
    Ok(match values.get(&base) {
        Some(grant) => {
            serde_json::json!({"server":base,"source":"saved_login","logged_in":grant.expires_at>chrono::Utc::now().timestamp(),"expires_at":grant.expires_at,"permission":grant.permission,"spaces":grant.spaces,"lists":grant.lists,"all_resources":grant.all_resources})
        }
        None => serde_json::json!({"server":base,"logged_in":false}),
    })
}
