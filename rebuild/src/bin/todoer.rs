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
    /// Workspace administration. JSON write bodies require current revisions.
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    /// List details, creation, sharing and access administration.
    Lists {
        #[command(subcommand)]
        command: ListCommand,
    },
    /// List section administration.
    Sections {
        #[command(subcommand)]
        command: SectionCommand,
    },
    /// Explain how to obtain a REST credential without copying browser cookies.
    AuthInfo,
    /// Review and atomically apply bounded, server-stored changes.
    Bulk {
        #[command(subcommand)]
        command: BulkCommand,
    },
    /// Resolve exact destinations and manage explicit accessible defaults.
    Destination {
        #[command(subcommand)]
        command: DestinationCommand,
    },
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
    TrashList,
    Trash {
        id: String,
        #[arg(long)]
        revision: i64,
    },
    Restore {
        id: String,
        #[arg(long)]
        revision: i64,
    },
    Undo {
        seq: i64,
        #[arg(long)]
        revision: i64,
    },
    Capabilities,
    Changes {
        #[arg(long, default_value_t = 0)]
        after: i64,
    },
}

/// --body is a JSON object matching the REST schema; revision and explicit
/// access-change acknowledgement remain visible in every write request.
#[derive(Subcommand)]
enum WorkspaceCommand {
    All,
    Get {
        id: String,
    },
    Create {
        #[arg(long)]
        body: String,
    },
    Edit {
        id: String,
        #[arg(long)]
        body: String,
    },
    Members {
        id: String,
    },
    Grant {
        id: String,
        #[arg(long)]
        body: String,
    },
    Revoke {
        id: String,
        #[arg(long)]
        body: String,
    },
    Reorder {
        id: String,
        #[arg(long)]
        body: String,
    },
}
#[derive(Subcommand)]
enum ListCommand {
    All,
    Get {
        id: String,
    },
    Create {
        #[arg(long)]
        body: String,
    },
    Edit {
        id: String,
        #[arg(long)]
        body: String,
    },
    Sharing {
        id: String,
    },
    Access {
        id: String,
        #[arg(long)]
        body: String,
    },
    Grant {
        id: String,
        #[arg(long)]
        body: String,
    },
    Revoke {
        id: String,
        #[arg(long)]
        body: String,
    },
}
#[derive(Subcommand)]
enum SectionCommand {
    All {
        list: String,
    },
    Create {
        list: String,
        #[arg(long)]
        body: String,
    },
    Edit {
        id: String,
        #[arg(long)]
        body: String,
    },
    Reorder {
        list: String,
        #[arg(long)]
        body: String,
    },
}
#[derive(Subcommand)]
enum BulkCommand {
    Preview {
        #[arg(long)]
        body: String,
    },
    Apply {
        preview_id: String,
        #[arg(long)]
        hash: String,
        #[arg(long)]
        key: String,
    },
}
#[derive(Subcommand)]
enum DestinationCommand {
    Resolve {
        #[arg(long)]
        destination: Option<String>,
        #[arg(long)]
        preference_key: Option<String>,
        #[arg(long)]
        fallback_inbox: bool,
    },
    Preferences,
    Set {
        key: String,
        list: String,
    },
    Search {
        #[arg(long, default_value = "")]
        prefix: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}
fn bulk_request(command: BulkCommand) -> Result<RequestSpec, (i32, String)> {
    match command {
        BulkCommand::Preview { body } => write_request(Method::POST, "/bulk/preview".into(), body),
        BulkCommand::Apply {
            preview_id,
            hash,
            key,
        } => Ok((
            Method::POST,
            "/bulk/apply".into(),
            Some(json!({"preview_id":preview_id,"expected_hash":hash,"idempotency_key":key})),
            None,
        )),
    }
}
fn destination_request(
    command: DestinationCommand,
    query: &mut Vec<(String, String)>,
) -> RequestSpec {
    match command {
        DestinationCommand::Resolve {
            destination,
            preference_key,
            fallback_inbox,
        } => {
            if let Some(value) = destination {
                query.push(("destination".into(), value));
            }
            if let Some(value) = preference_key {
                query.push(("default_key".into(), value));
            }
            query.push(("fallback_inbox".into(), fallback_inbox.to_string()));
            read_request("/destinations/resolve".into())
        }
        DestinationCommand::Preferences => read_request("/destinations/preferences".into()),
        DestinationCommand::Set { key, list } => (
            Method::POST,
            "/destinations/preferences".into(),
            Some(json!({"key":key,"list_id":list})),
            None,
        ),
        DestinationCommand::Search { prefix, limit } => {
            query.push(("prefix".into(), prefix));
            query.push(("limit".into(), limit.to_string()));
            read_request("/destinations/search".into())
        }
    }
}
type RequestSpec = (Method, String, Option<Value>, Option<String>);
fn read_request(path: String) -> RequestSpec {
    (Method::GET, path, None, None)
}
fn write_request(method: Method, path: String, body: String) -> Result<RequestSpec, (i32, String)> {
    let value: Value =
        serde_json::from_str(&body).map_err(|_| (2, "--body must contain valid JSON".into()))?;
    if !value.is_object() {
        return Err((2, "--body must be a JSON object".into()));
    }
    Ok((method, path, Some(value), None))
}
fn workspace_request(command: WorkspaceCommand) -> Result<RequestSpec, (i32, String)> {
    Ok(match command {
        WorkspaceCommand::All => read_request("/workspaces".into()),
        WorkspaceCommand::Get { id } => read_request(format!("/workspaces/{}", segment(&id))),
        WorkspaceCommand::Create { body } => {
            write_request(Method::POST, "/workspaces".into(), body)?
        }
        WorkspaceCommand::Edit { id, body } => {
            write_request(Method::PATCH, format!("/workspaces/{}", segment(&id)), body)?
        }
        WorkspaceCommand::Members { id } => {
            read_request(format!("/workspaces/{}/members", segment(&id)))
        }
        WorkspaceCommand::Grant { id, body } => write_request(
            Method::POST,
            format!("/workspaces/{}/members", segment(&id)),
            body,
        )?,
        WorkspaceCommand::Revoke { id, body } => write_request(
            Method::POST,
            format!("/workspaces/{}/members/revoke", segment(&id)),
            body,
        )?,
        WorkspaceCommand::Reorder { id, body } => write_request(
            Method::POST,
            format!("/workspaces/{}/lists/reorder", segment(&id)),
            body,
        )?,
    })
}
fn list_request(command: ListCommand) -> Result<RequestSpec, (i32, String)> {
    Ok(match command {
        ListCommand::All => read_request("/lists".into()),
        ListCommand::Get { id } => read_request(format!("/lists/{}", segment(&id))),
        ListCommand::Create { body } => write_request(Method::POST, "/lists".into(), body)?,
        ListCommand::Edit { id, body } => {
            write_request(Method::PATCH, format!("/lists/{}", segment(&id)), body)?
        }
        ListCommand::Sharing { id } => read_request(format!("/lists/{}/sharing", segment(&id))),
        ListCommand::Access { id, body } => write_request(
            Method::POST,
            format!("/lists/{}/access", segment(&id)),
            body,
        )?,
        ListCommand::Grant { id, body } => write_request(
            Method::POST,
            format!("/lists/{}/grants", segment(&id)),
            body,
        )?,
        ListCommand::Revoke { id, body } => write_request(
            Method::POST,
            format!("/lists/{}/grants/revoke", segment(&id)),
            body,
        )?,
    })
}
fn section_request(command: SectionCommand) -> Result<RequestSpec, (i32, String)> {
    Ok(match command {
        SectionCommand::All { list } => read_request(format!("/lists/{}/sections", segment(&list))),
        SectionCommand::Create { list, body } => write_request(
            Method::POST,
            format!("/lists/{}/sections", segment(&list)),
            body,
        )?,
        SectionCommand::Edit { id, body } => {
            write_request(Method::PATCH, format!("/sections/{}", segment(&id)), body)?
        }
        SectionCommand::Reorder { list, body } => write_request(
            Method::POST,
            format!("/lists/{}/sections/reorder", segment(&list)),
            body,
        )?,
    })
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
    if matches!(&args.command, Command::AuthInfo) {
        let info = json!({"authentication":"REST bearer credential", "settings":"/settings", "instructions":"Sign in using the browser, create a REST credential with the required permissions and lists in /settings, and set TODOER_TOKEN. Browser-only credential issuance and revocation remain in Settings."});
        if args.json {
            println!("{info}");
        } else {
            println!("{}", info["instructions"].as_str().unwrap());
        }
        return Ok(());
    }
    let mut query_pairs: Vec<(String, String)> = Vec::new();
    let (method, path, body, key) = match args.command {
        Command::Workspace { command } => workspace_request(command)?,
        Command::Lists { command } => list_request(command)?,
        Command::Sections { command } => section_request(command)?,
        Command::AuthInfo => unreachable!("handled before request construction"),
        Command::Bulk { command } => bulk_request(command)?,
        Command::Destination { command } => destination_request(command, &mut query_pairs),
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
        Command::TrashList => (Method::GET, "/trash".into(), None, None),
        Command::Trash { id, revision } => (
            Method::POST,
            format!("/tasks/{}/trash", segment(&id)),
            Some(json!({"revision": revision})),
            None,
        ),
        Command::Restore { id, revision } => (
            Method::POST,
            format!("/tasks/{}/restore", segment(&id)),
            Some(json!({"revision": revision})),
            None,
        ),
        Command::Undo { seq, revision } => (
            Method::POST,
            format!("/history/{seq}/undo"),
            Some(json!({"revision": revision})),
            None,
        ),
        Command::Capabilities => (Method::GET, "/capabilities".into(), None, None),
        Command::Changes { after } => (Method::GET, format!("/changes?after={after}"), None, None),
    };
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
    let token = std::env::var("TODOER_TOKEN")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or((3, "TODOER_TOKEN must be set and nonempty".into()))?;
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
            _ => match (
                parsed.get("error").and_then(Value::as_str),
                parsed.get("error_description").and_then(Value::as_str),
            ) {
                (Some(code), Some(message)) => format!("{code}: {message}"),
                _ => format!("server returned HTTP {}", status.as_u16()),
            },
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
