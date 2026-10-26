use clap::{Parser, Subcommand};
use reqwest::{Client, Method};
use serde_json::{Value, json};

#[derive(Parser)]
#[command(about = "Todoer REST client")]
struct Args {
    #[arg(
        long,
        env = "TODOER_URL",
        default_value = "http://127.0.0.1:8092",
        global = true
    )]
    url: String,
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    List,
    Tasks {
        list: String,
    },
    Search {
        #[arg(long, default_value = "")]
        query: String,
        #[arg(long)]
        list: Option<String>,
        #[arg(long)]
        complete: Option<bool>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long)]
        after: Option<String>,
    },
    Capture {
        #[arg(long)]
        list: String,
        #[arg(long)]
        key: String,
        #[arg(required = true)]
        titles: Vec<String>,
    },
    Edit {
        id: String,
        #[arg(long)]
        revision: i64,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        description: Option<String>,
    },
    Complete {
        id: String,
        #[arg(long)]
        revision: i64,
        #[arg(long)]
        subtree: bool,
    },
    Reopen {
        id: String,
        #[arg(long)]
        revision: i64,
    },
    Subtree {
        id: String,
    },
    Move {
        id: String,
        #[arg(long)]
        destination: String,
        #[arg(long)]
        revision: i64,
        #[arg(long)]
        acknowledge_access_change: bool,
    },
    MovePreview {
        id: String,
        #[arg(long)]
        destination: String,
    },
    Changes {
        #[arg(long, default_value_t = 0)]
        after: i64,
    },
}

fn segment(value: &str) -> String {
    // Encode user-provided identifiers as a single URL path segment.
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    if let Err((exit, message)) = run(args).await {
        eprintln!("{message}");
        std::process::exit(exit);
    }
}

async fn run(args: Args) -> Result<(), (i32, String)> {
    let mut query_pairs: Vec<(String, String)> = Vec::new();
    let (method, path, body, key) = match args.command {
        Command::List => (Method::GET, "/lists".into(), None, None),
        Command::Tasks { list } => (
            Method::GET,
            format!("/lists/{}/tasks", segment(&list)),
            None,
            None,
        ),
        Command::Search {
            query,
            list,
            complete,
            limit,
            after,
        } => {
            query_pairs.push(("query".into(), query));
            query_pairs.push(("limit".into(), limit.to_string()));
            if let Some(list) = list {
                query_pairs.push(("list".into(), list));
            }
            if let Some(complete) = complete {
                query_pairs.push(("complete".into(), complete.to_string()));
            }
            if let Some(after) = after {
                query_pairs.push(("after".into(), after));
            }
            (Method::GET, "/search".into(), None, None)
        }
        Command::Capture { list, key, titles } => (
            Method::POST,
            "/capture".into(),
            Some(
                json!({"list_id": list, "tasks": titles.into_iter().map(|title| json!({"title": title, "description": "", "parent_id": null})).collect::<Vec<_>>()}),
            ),
            Some(key),
        ),
        Command::Edit {
            id,
            revision,
            title,
            description,
        } => {
            if title.is_none() && description.is_none() {
                return Err((2, "edit requires --title or --description".into()));
            }
            let mut body = json!({"revision": revision});
            if let Some(title) = title {
                body["title"] = json!(title);
            }
            if let Some(description) = description {
                body["description"] = json!(description);
            }
            (
                Method::PATCH,
                format!("/tasks/{}", segment(&id)),
                Some(body),
                None,
            )
        }
        Command::Complete {
            id,
            revision,
            subtree,
        } => (
            Method::PATCH,
            format!("/tasks/{}", segment(&id)),
            Some(json!({"revision": revision, "complete": true, "complete_subtree": subtree})),
            None,
        ),
        Command::Reopen { id, revision } => (
            Method::PATCH,
            format!("/tasks/{}", segment(&id)),
            Some(json!({"revision": revision, "complete": false})),
            None,
        ),
        Command::Subtree { id } => (
            Method::GET,
            format!("/tasks/{}/subtree", segment(&id)),
            None,
            None,
        ),
        Command::Move {
            id,
            destination,
            revision,
            acknowledge_access_change,
        } => (
            Method::POST,
            format!("/tasks/{}/move", segment(&id)),
            Some(
                json!({"destination": destination, "revision": revision, "acknowledge_access_change": acknowledge_access_change}),
            ),
            None,
        ),
        Command::MovePreview { id, destination } => {
            query_pairs.push(("destination".into(), destination));
            (
                Method::GET,
                format!("/tasks/{}/move-preview", segment(&id)),
                None,
                None,
            )
        }
        Command::Changes { after } => (Method::GET, format!("/changes?after={after}"), None, None),
    };
    let token =
        std::env::var("TODOER_TOKEN").map_err(|_| (3, "TODOER_TOKEN must be set".into()))?;
    if token.trim().is_empty() {
        return Err((3, "TODOER_TOKEN must not be empty".into()));
    }
    // Do not forward the bearer credential across redirects.
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|_| (1, "could not initialize HTTP client".into()))?;
    let mut url = reqwest::Url::parse(&format!("{}/api/v1{path}", args.url.trim_end_matches('/')))
        .map_err(|_| (2, "invalid server URL".into()))?;
    if !query_pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(query_pairs);
    }
    let mut request = client.request(method, url).bearer_auth(token);
    if let Some(body) = body {
        request = request.json(&body);
    }
    if let Some(key) = key {
        request = request.header("Idempotency-Key", key);
    }
    let response = request.send().await.map_err(|_| {
        (
            1,
            "request failed; check server URL and connectivity".into(),
        )
    })?;
    let status = response.status();
    let raw = response
        .text()
        .await
        .map_err(|_| (1, "could not read server response".into()))?;
    if !status.is_success() {
        let exit = match status.as_u16() {
            401 | 403 => 3,
            400 | 422 => 2,
            409 | 412 => 4,
            _ => 1,
        };
        let parsed: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
        let message = match (
            parsed.pointer("/error/code").and_then(Value::as_str),
            parsed.pointer("/error/message").and_then(Value::as_str),
        ) {
            (Some(code), Some(message)) => format!("{code}: {message}"),
            _ => format!("server returned HTTP {}", status.as_u16()),
        };
        return Err((exit, message));
    }
    if args.json {
        println!("{raw}");
    } else if raw.is_empty() {
        println!("OK");
    } else {
        let value: Value =
            serde_json::from_str(&raw).map_err(|_| (1, "server returned invalid JSON".into()))?;
        print_human(&value);
    }
    Ok(())
}

fn print_human(value: &Value) {
    if let Some(rows) = value.as_array() {
        if rows.is_empty() {
            println!("No results.");
        }
        for row in rows {
            print_human(row);
        }
    } else if value.get("title").is_some() {
        let id = value
            .get("reference")
            .or_else(|| value.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("?");
        let title = value["title"].as_str().unwrap_or("");
        let marker = match value.get("complete").and_then(Value::as_bool) {
            Some(true) => "[x] ",
            Some(false) => "[ ] ",
            None => "",
        };
        println!("{marker}{id}  {title}  (revision {})", value["revision"]);
        if let Some(description) = value
            .get("description")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            println!("    {description}");
        }
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(value).expect("JSON value serializes")
        );
    }
}
