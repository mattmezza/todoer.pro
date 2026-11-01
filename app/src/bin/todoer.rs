#[path = "cli_auth/mod.rs"]
mod cli_login;
use clap::{Parser, Subcommand};
use reqwest::{Client, Method};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

#[derive(Parser)]
#[command(about = "Todoer REST client", version = env!("TODOER_BUILD_VERSION"))]
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
    /// Weekly review with live permissions and counting/coverage notes.
    Review {
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        to: Option<String>,
        #[arg(long)]
        list: Option<String>,
        #[arg(long = "space", visible_alias = "workspace")]
        workspace: Option<String>,
    },
    /// Space administration. JSON write bodies require current revisions.
    #[command(name = "space", visible_alias = "workspace")]
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    /// View a space, list, section, task or subtask by its prefixed reference.
    View {
        reference: String,
        /// Show descriptions and child counts.
        #[arg(long)]
        info: bool,
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
    #[command(name = "auth", alias = "auth-info")]
    AuthInfo,
    /// Sign in using your browser; approve permissions and selected spaces/lists.
    Login {
        #[arg(long, default_value="write", value_parser=["read","write","admin"])]
        permission: String,
        #[arg(long)]
        no_browser: bool,
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },
    /// Revoke and remove the saved login for this server.
    Logout,
    /// Show the saved login or environment credential source, without secrets.
    AuthStatus,
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
    /// Structured authorized search, with explicit status, scope, dates and projection.
    Query(Box<QueryArgs>),
    /// Compact organization action JSON (see OpenAPI OrganizeAction).
    Organize {
        #[arg(long)]
        body: String,
    },
    Tags {
        #[command(subcommand)]
        command: TagCommand,
    },
    TaskSection {
        id: String,
        #[arg(long)]
        revision: Option<i64>,
        #[arg(long)]
        section: Option<String>,
        #[arg(long)]
        clear: bool,
    },
    Reparent {
        id: String,
        #[arg(long)]
        revision: i64,
        #[arg(long)]
        parent: Option<String>,
    },
    Reorder {
        list: String,
        #[arg(long)]
        revision: i64,
        #[arg(long)]
        parent: Option<String>,
        ids: Vec<String>,
    },
    References {
        #[command(subcommand)]
        command: ReferenceCommand,
    },
    /// Scheduling and recurrence through shared operations.
    Schedule {
        #[command(subcommand)]
        command: ScheduleCommand,
    },
    Comments {
        #[command(subcommand)]
        command: CommentCommand,
    },
    /// Metadata plus bounded checksum-verified file upload/download.
    Files {
        #[command(subcommand)]
        command: FileCommand,
    },
    /// Compact access action JSON; grants require explicit acknowledgment.
    Access {
        #[arg(long)]
        body: String,
    },
    Activity {
        #[arg(long)]
        body: String,
    },
    Invitations {
        #[command(subcommand)]
        command: InvitationCommand,
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
    /// Bounded projected subtree page (default50/max100); opt into description.
    Subtree {
        id: String,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long, value_delimiter = ',')]
        fields: Vec<String>,
        #[arg(long)]
        cursor: Option<String>,
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
    /// Review audience and tag changes before moving the entire list.
    MovePreview {
        id: String,
        #[arg(long = "destination-space", visible_alias = "destination-workspace")]
        destination_workspace: String,
    },
    /// Apply the exact reviewed revisions and policy hash (ListWorkspaceMoveInput JSON).
    Move {
        id: String,
        #[arg(long)]
        body: String,
    },
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
#[derive(clap::Args)]
struct QueryArgs {
    /// Complete canonical JSON query; exclusive with filter flags.
    #[arg(long, conflicts_with_all=["text","entity","workspace","list","section","tag","status","priority","planned_from","planned_to","due_from","due_to","recurrence","limit","cursor","sort","field"]) ]
    body: Option<String>,
    #[arg(long, default_value = "")]
    text: String,
    #[arg(long)]
    entity: Vec<String>,
    #[arg(long = "space", visible_alias = "workspace")]
    workspace: Option<String>,
    #[arg(long)]
    list: Option<String>,
    #[arg(long)]
    section: Option<String>,
    #[arg(long)]
    tag: Option<String>,
    #[arg(long, default_value = "open")]
    status: String,
    #[arg(long)]
    priority: Option<u8>,
    #[arg(long)]
    planned_from: Option<String>,
    #[arg(long)]
    planned_to: Option<String>,
    #[arg(long)]
    due_from: Option<String>,
    #[arg(long)]
    due_to: Option<String>,
    #[arg(long)]
    recurrence: Option<String>,
    #[arg(long, default_value_t = 50)]
    limit: usize,
    #[arg(long)]
    cursor: Option<String>,
    #[arg(long, default_value = "relevance")]
    sort: String,
    #[arg(long)]
    field: Vec<String>,
}
#[derive(Subcommand)]
enum TagCommand {
    List {
        list: String,
        #[arg(long, default_value = "")]
        prefix: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    Create {
        list: String,
        name: String,
    },
    Rename {
        id: String,
        #[arg(long)]
        revision: i64,
        name: String,
    },
    Get {
        task: String,
    },
    Set {
        task: String,
        #[arg(long)]
        revision: i64,
        #[arg(long)]
        tag: Vec<String>,
        #[arg(long)]
        tag_id: Vec<String>,
    },
}
#[derive(Subcommand)]
enum ReferenceCommand {
    Resolve {
        reference: String,
    },
    Autocomplete {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    Unresolved {
        kind: String,
        id: String,
    },
    Bind {
        kind: String,
        id: String,
        #[arg(long)]
        revision: i64,
        #[arg(long)]
        occurrence: usize,
        #[arg(long)]
        target_kind: String,
        #[arg(long)]
        target_id: String,
    },
    Backlinks {
        kind: String,
        id: String,
    },
}
fn query_request(q: QueryArgs) -> Result<RequestSpec, (i32, String)> {
    if let Some(body) = q.body {
        return write_request(Method::POST, "/query".into(), body);
    }
    let entities = if q.entity.is_empty() {
        None
    } else {
        Some(q.entity)
    };
    let fields = if q.field.is_empty() {
        None
    } else {
        Some(q.field)
    };
    Ok((
        Method::POST,
        "/query".into(),
        Some(
            json!({"text":q.text,"entities":entities,"workspace_id":q.workspace.as_deref().map(unprefix),"list_id":q.list.as_deref().map(unprefix),"section_id":q.section.as_deref().map(unprefix),"tag":q.tag,"status":q.status,"priority":q.priority,"planned_from":q.planned_from,"planned_to":q.planned_to,"due_from":q.due_from,"due_to":q.due_to,"recurrence":q.recurrence,"limit":q.limit,"cursor":q.cursor,"sort":q.sort,"fields":fields}),
        ),
        None,
    ))
}
fn action_request(path: &str, body: Value) -> RequestSpec {
    (Method::POST, path.into(), Some(body), None)
}
fn tag_request(command: TagCommand) -> RequestSpec {
    action_request(
        "/organize",
        match command {
            TagCommand::List {
                list,
                prefix,
                limit,
            } => json!({"op":"tag_list","list_id":list,"prefix":prefix,"limit":limit}),
            TagCommand::Create { list, name } => {
                json!({"op":"tag_create","list_id":list,"name":name})
            }
            TagCommand::Rename { id, revision, name } => {
                json!({"op":"tag_rename","id":id,"revision":revision,"name":name})
            }
            TagCommand::Get { task } => json!({"op":"tags_get","id":task}),
            TagCommand::Set {
                task,
                revision,
                tag,
                tag_id,
            } => {
                let mut tags: Vec<Value> = tag.into_iter().map(Value::String).collect();
                tags.extend(tag_id.into_iter().map(|id| json!({"id":id})));
                json!({"op":"tags_set","id":task,"revision":revision,"tags":tags})
            }
        },
    )
}
fn reference_request(command: ReferenceCommand) -> RequestSpec {
    action_request(
        "/references",
        match command {
            ReferenceCommand::Resolve { reference } => {
                json!({"op":"resolve","reference":reference})
            }
            ReferenceCommand::Autocomplete { query, limit } => {
                json!({"op":"autocomplete","query":query,"limit":limit})
            }
            ReferenceCommand::Unresolved { kind, id } => {
                json!({"op":"unresolved","source_kind":kind,"source_id":id})
            }
            ReferenceCommand::Bind {
                kind,
                id,
                revision,
                occurrence,
                target_kind,
                target_id,
            } => {
                json!({"op":"bind","source_kind":kind,"source_id":id,"revision":revision,"occurrence":occurrence,"target_kind":target_kind,"target_id":target_id})
            }
            ReferenceCommand::Backlinks { kind, id } => {
                json!({"op":"backlinks","target_kind":kind,"target_id":id})
            }
        },
    )
}
#[derive(Subcommand)]
enum ScheduleCommand {
    Get {
        id: String,
    },
    Set {
        id: String,
        #[arg(long)]
        revision: i64,
        #[arg(long)]
        body: String,
    },
    Start {
        id: String,
        #[arg(long)]
        revision: i64,
        #[arg(long)]
        body: String,
    },
    Series {
        id: String,
    },
    All,
    State {
        id: String,
        #[arg(long)]
        revision: i64,
        state: String,
    },
    Occurrences {
        id: String,
    },
    Skip {
        id: String,
        #[arg(long)]
        revision: i64,
    },
    Planning {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
    },
    Statistics {
        #[arg(long)]
        from: String,
    },
    Action {
        #[arg(long)]
        body: String,
    },
}
#[derive(Subcommand)]
enum CommentCommand {
    List {
        task: String,
        #[arg(long, default_value_t = 0)]
        after: i64,
        #[arg(long, default_value_t = 20)]
        limit: i64,
        #[arg(long)]
        full: bool,
    },
    Get {
        id: String,
    },
    Add {
        task: String,
        #[arg(long)]
        key: String,
        body: String,
    },
    Edit {
        id: String,
        #[arg(long)]
        revision: i64,
        body: String,
    },
    Delete {
        id: String,
        #[arg(long)]
        revision: i64,
    },
}
#[derive(Subcommand)]
enum FileCommand {
    List {
        task: String,
    },
    Info {
        id: String,
    },
    Delete {
        id: String,
        #[arg(long)]
        revision: i64,
    },
    Intent {
        task: String,
        #[arg(long)]
        body: String,
    },
    ResumeIntent {
        id: String,
    },
    #[command(alias = "confirm")]
    Finalize {
        id: String,
        #[arg(long)]
        revision: i64,
    },
    Upload {
        task: String,
        #[arg(long)]
        file: std::path::PathBuf,
        #[arg(long)]
        comment: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    Resume {
        id: String,
        #[arg(long)]
        file: std::path::PathBuf,
    },
    Download {
        id: String,
        #[arg(long)]
        output: std::path::PathBuf,
    },
}
#[derive(Subcommand)]
enum InvitationCommand {
    List {
        kind: String,
        id: String,
    },
    Create {
        kind: String,
        id: String,
        email: String,
        #[arg(long, default_value = "viewer")]
        role: String,
        #[arg(long)]
        key: String,
        #[arg(long)]
        acknowledge_access_change: bool,
    },
    Revoke {
        id: String,
    },
}
fn object_body(body: &str) -> Result<Value, (i32, String)> {
    let value: Value =
        serde_json::from_str(body).map_err(|_| (2, "--body must contain valid JSON".into()))?;
    if !value.is_object() {
        return Err((2, "--body must contain a JSON object".into()));
    }
    Ok(value)
}
fn schedule_request(command: ScheduleCommand) -> Result<RequestSpec, (i32, String)> {
    Ok(action_request(
        "/agent/schedule",
        match command {
            ScheduleCommand::Get { id } => json!({"op":"get","id":id}),
            ScheduleCommand::Set { id, revision, body } => {
                json!({"op":"set","id":id,"revision":revision,"schedule":object_body(&body)?})
            }
            ScheduleCommand::Start { id, revision, body } => {
                json!({"op":"series_start","id":id,"revision":revision,"spec":object_body(&body)?})
            }
            ScheduleCommand::Series { id } => json!({"op":"series_get","id":id}),
            ScheduleCommand::All => json!({"op":"series_list"}),
            ScheduleCommand::State {
                id,
                revision,
                state,
            } => json!({"op":"series_state","id":id,"revision":revision,"state":state}),
            ScheduleCommand::Occurrences { id } => json!({"op":"occurrences","id":id}),
            ScheduleCommand::Skip { id, revision } => {
                json!({"op":"skip","id":id,"revision":revision})
            }
            ScheduleCommand::Planning { from, to } => json!({"op":"planning","from":from,"to":to}),
            ScheduleCommand::Statistics { from } => json!({"op":"statistics","from":from}),
            ScheduleCommand::Action { body } => object_body(&body)?,
        },
    ))
}
fn comments_request(command: CommentCommand) -> RequestSpec {
    action_request(
        "/agent/activity",
        match command {
            CommentCommand::List {
                task,
                after,
                limit,
                full,
            } => {
                json!({"op":"comments","task_id":task,"after_seq":after,"limit":limit,"include_bodies":full})
            }
            CommentCommand::Get { id } => json!({"op":"comment_get","id":id}),
            CommentCommand::Add { task, key, body } => {
                json!({"op":"comment_create","task_id":task,"idempotency_key":key,"body":body})
            }
            CommentCommand::Edit { id, revision, body } => {
                json!({"op":"comment_edit","id":id,"revision":revision,"body":body})
            }
            CommentCommand::Delete { id, revision } => {
                json!({"op":"comment_delete","id":id,"revision":revision})
            }
        },
    )
}
fn files_request(command: FileCommand) -> Result<RequestSpec, (i32, String)> {
    Ok(action_request(
        "/agent/activity",
        match command {
            FileCommand::List { task } => json!({"op":"files","task_id":task}),
            FileCommand::Info { id } => json!({"op":"file_link","id":id}),
            FileCommand::Delete { id, revision } => {
                json!({"op":"file_delete","id":id,"revision":revision})
            }
            FileCommand::Intent { task, body } => {
                json!({"op":"upload_create","task_id":task,"input":object_body(&body)?})
            }
            FileCommand::ResumeIntent { id } => json!({"op":"upload_get","id":id}),
            FileCommand::Finalize { id, revision } => {
                json!({"op":"upload_finalize","id":id,"revision":revision})
            }
            FileCommand::Upload { .. }
            | FileCommand::Resume { .. }
            | FileCommand::Download { .. } => {
                unreachable!("binary transfer handled before request dispatch")
            }
        },
    ))
}
fn invitation_request(command: InvitationCommand) -> RequestSpec {
    action_request(
        "/agent/access",
        match command {
            InvitationCommand::List { kind, id } => {
                let kind = if kind == "space" {
                    "workspace".to_owned()
                } else {
                    kind
                };
                json!({"op":"invitations","target_kind":kind,"target_id":id})
            }
            InvitationCommand::Create {
                kind,
                id,
                email,
                role,
                key,
                acknowledge_access_change,
            } => {
                let kind = if kind == "space" {
                    "workspace".to_owned()
                } else {
                    kind
                };
                json!({"op":"invite","idempotency_key":key,"input":{"target_kind":kind,"target_id":id,"email":email,"role":role,"acknowledge_access_change":acknowledge_access_change}})
            }
            InvitationCommand::Revoke { id } => json!({"op":"revoke_invitation","id":id}),
        },
    )
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
        ListCommand::MovePreview {
            id,
            destination_workspace,
        } => (
            Method::POST,
            format!("/lists/{}/workspace-move/preview", segment(&id)),
            Some(json!({"destination_workspace_id":destination_workspace})),
            None,
        ),
        ListCommand::Move { id, body } => write_request(
            Method::POST,
            format!("/lists/{}/workspace-move", segment(&id)),
            body,
        )?,
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
    unprefix(value)
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

fn unprefix(value: &str) -> &str {
    value
        .strip_prefix(['@', '#', '$', '%', '^'])
        .unwrap_or(value)
}

fn view_request(reference: &str) -> Result<RequestSpec, (i32, String)> {
    let (prefix, id) = reference
        .split_at_checked(1)
        .ok_or((2, "expected a prefixed reference (@, #, $, % or ^)".into()))?;
    if id.is_empty() {
        return Err((2, "reference is missing its identifier".into()));
    }
    let path = match prefix {
        "@" => "space",
        "#" => "list",
        "$" => "section",
        "%" => "task",
        "^" => "subtask",
        _ => return Err((2, "expected a prefixed reference (@, #, $, % or ^)".into())),
    };
    Ok(read_request(format!("/view/{path}/{}", segment(id))))
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
    match &args.command {
        Command::Login {
            permission,
            no_browser,
            timeout,
        } => {
            let value = cli_login::login(&args.url, permission, *no_browser, *timeout).await?;
            output_value(&args, &value);
            return Ok(());
        }
        Command::Logout => {
            let value = cli_login::logout(&args.url).await?;
            output_value(&args, &value);
            return Ok(());
        }
        Command::AuthStatus => {
            let value = cli_login::status(&args.url)?;
            output_value(&args, &value);
            return Ok(());
        }
        _ => {}
    }
    if matches!(&args.command, Command::AuthInfo) {
        let info = json!({"authentication":"REST bearer credential", "settings":"/settings", "instructions":"Run todoer login --permission read|write|admin, then approve spaces/lists in your browser. Saved credentials are used automatically. TODOER_TOKEN overrides the saved login. Use todoer auth-status or todoer logout."});
        if args.json {
            println!("{info}");
        } else {
            println!("{}", info["instructions"].as_str().unwrap());
        }
        return Ok(());
    }
    if let Command::Files { command } = &args.command
        && matches!(
            command,
            FileCommand::Upload { .. } | FileCommand::Resume { .. } | FileCommand::Download { .. }
        )
    {
        return transfer_file(&args, command).await;
    }
    let listing_prefix = match &args.command {
        Command::List
        | Command::Lists {
            command: ListCommand::All,
        } => Some('#'),
        Command::Workspace {
            command: WorkspaceCommand::All,
        } => Some('@'),
        Command::Sections {
            command: SectionCommand::All { .. },
        } => Some('$'),
        Command::Tasks { .. } | Command::TrashList => Some('%'),
        _ => None,
    };
    let compact_subtree =
        matches!(&args.command, Command::Subtree { fields, .. } if fields.is_empty());
    let view_info = match &args.command {
        Command::View { info, .. } => Some(*info),
        _ => None,
    };
    let mut query_pairs: Vec<(String, String)> = Vec::new();
    let (method, path, body, key) = match args.command {
        Command::Review {
            from,
            to,
            list,
            workspace,
        } => {
            for (key, value) in [
                ("from", from),
                ("to", to),
                ("list", list),
                ("workspace", workspace),
            ] {
                if let Some(value) = value {
                    query_pairs.push((
                        key.into(),
                        if key == "workspace" || key == "list" {
                            unprefix(&value).to_owned()
                        } else {
                            value
                        },
                    ));
                }
            }
            read_request("/review".into())
        }
        Command::Workspace { command } => workspace_request(command)?,
        Command::View { reference, .. } => view_request(&reference)?,
        Command::Lists { command } => list_request(command)?,
        Command::Sections { command } => section_request(command)?,
        Command::AuthInfo | Command::Login { .. } | Command::Logout | Command::AuthStatus => {
            unreachable!("handled before request construction")
        }
        Command::Bulk { command } => bulk_request(command)?,
        Command::Query(q) => query_request(*q)?,
        Command::Schedule { command } => schedule_request(command)?,
        Command::Comments { command } => comments_request(command),
        Command::Files { command } => files_request(command)?,
        Command::Access { body } => write_request(Method::POST, "/agent/access".into(), body)?,
        Command::Activity { body } => write_request(Method::POST, "/agent/activity".into(), body)?,
        Command::Invitations { command } => invitation_request(command),
        Command::Organize { body } => write_request(Method::POST, "/organize".into(), body)?,
        Command::Tags { command } => tag_request(command),
        Command::TaskSection {
            id,
            revision,
            section,
            clear,
        } => {
            if section.is_some() && clear {
                return Err((2, "--section and --clear are mutually exclusive".into()));
            }
            if let Some(revision) = revision {
                if section.is_none() && !clear {
                    return Err((
                        2,
                        "set --section or --clear when supplying --revision".into(),
                    ));
                }
                action_request(
                    "/organize",
                    json!({"op":"section_set","id":id,"revision":revision,"section_id":section}),
                )
            } else {
                if section.is_some() || clear {
                    return Err((2, "--revision is required when changing section".into()));
                }
                action_request("/organize", json!({"op":"section_get","id":id}))
            }
        }
        Command::Reparent {
            id,
            revision,
            parent,
        } => action_request(
            "/organize",
            json!({"op":"reparent","id":id,"revision":revision,"parent_id":parent}),
        ),
        Command::Reorder {
            list,
            revision,
            parent,
            ids,
        } => action_request(
            "/organize",
            json!({"op":"reorder","list_id":list,"revision":revision,"parent_id":parent,"ids":ids}),
        ),
        Command::References { command } => reference_request(command),
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
                json!({"list_id": unprefix(&list), "tasks": titles.into_iter().map(|title| json!({"title": title, "description": "", "parent_id": null})).collect::<Vec<_>>()}),
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
        Command::Subtree {
            id,
            limit,
            fields,
            cursor,
        } => {
            let mut body = json!({"limit":limit,"cursor":cursor});
            if fields.is_empty() && !args.json {
                body["fields"] = json!([
                    "reference",
                    "public_reference",
                    "title",
                    "complete",
                    "revision",
                    "position"
                ]);
            } else if !fields.is_empty() {
                body["fields"] = json!(fields);
            }
            (
                Method::POST,
                format!("/tasks/{}/subtree/query", segment(&id)),
                Some(body),
                None,
            )
        }
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
    let token = cli_login::token(&args.url)?;
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
    let (raw, value) = response_json(response).await?;
    if args.json {
        println!("{raw}");
    } else if raw.is_empty() {
        println!("OK");
    } else if let Some(prefix) = listing_prefix.filter(|_| value.is_array()) {
        print!("{}", format_lists(&value, prefix));
    } else if compact_subtree && value.get("items").and_then(Value::as_array).is_some() {
        print!("{}", format_subtree(&value));
    } else if let Some(info) = view_info {
        print!("{}", format_view(&value, info));
    } else {
        print_human(&value);
    }
    Ok(())
}

async fn response_json(response: reqwest::Response) -> Result<(String, Value), (i32, String)> {
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
    let value = if raw.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&raw).map_err(|_| (1, "server returned invalid JSON".into()))?
    };
    Ok((raw, value))
}
fn api_url(base: &str, path: &str) -> Result<reqwest::Url, (i32, String)> {
    reqwest::Url::parse(&format!("{}/api/v1{path}", base.trim_end_matches('/')))
        .map_err(|_| (2, "invalid server URL".into()))
}
async fn transfer_json(
    client: &Client,
    base: &str,
    token: &str,
    path: &str,
    body: Value,
) -> Result<Value, (i32, String)> {
    let response = client
        .post(api_url(base, path)?)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .map_err(|_| (1, "request failed; check server connectivity".into()))?;
    Ok(response_json(response).await?.1)
}
fn local_file(path: &std::path::Path) -> Result<(Vec<u8>, String), (i32, String)> {
    let file = std::fs::File::open(path).map_err(|_| (2, "could not open selected file".into()))?;
    if !file
        .metadata()
        .map_err(|_| (2, "could not inspect selected file".into()))?
        .is_file()
    {
        return Err((2, "selected path must be a regular file".into()));
    }
    let mut bytes = Vec::new();
    file.take(todoer::discussion::MAX_ATTACHMENT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| (2, "could not read selected file".into()))?;
    if bytes.len() as u64 > todoer::discussion::MAX_ATTACHMENT_BYTES {
        return Err((2, "file exceeds25MiB limit".into()));
    }
    let hash = format!("{:x}", Sha256::digest(&bytes));
    Ok((bytes, hash))
}
fn output_value(args: &Args, value: &Value) {
    if args.json {
        println!("{value}");
    } else {
        print_human(value);
    }
}
async fn transfer_file(args: &Args, command: &FileCommand) -> Result<(), (i32, String)> {
    let prepared = match command {
        FileCommand::Upload { file, .. } | FileCommand::Resume { file, .. } => {
            Some(local_file(file)?)
        }
        _ => None,
    };
    if let FileCommand::Upload {
        file,
        dry_run: true,
        ..
    } = command
    {
        let (bytes, hash) = prepared.as_ref().expect("prepared file");
        output_value(
            args,
            &json!({"dry_run":true,"filename":file.file_name().unwrap_or_default().to_string_lossy(),"size":bytes.len(),"sha256":hash}),
        );
        return Ok(());
    }
    let token = cli_login::token(&args.url)?;
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|_| (1, "could not initialize HTTP client".into()))?;
    if let FileCommand::Download { id, output } = command {
        let metadata = transfer_json(
            &client,
            &args.url,
            &token,
            "/agent/activity",
            json!({"op":"file_link","id":id}),
        )
        .await?;
        let expected_size = metadata["attachment"]["size"]
            .as_u64()
            .ok_or((1, "server omitted attachment size".into()))?;
        let expected_hash = metadata["attachment"]["sha256"]
            .as_str()
            .ok_or((1, "server omitted attachment checksum".into()))?;
        if expected_size > todoer::discussion::MAX_ATTACHMENT_BYTES {
            return Err((2, "attachment exceeds25MiB limit".into()));
        }
        let mut response = client
            .get(api_url(
                &args.url,
                &format!("/attachments/{}/download", segment(id)),
            )?)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|_| (1, "download failed".into()))?;
        if !response.status().is_success() {
            return response_json(response).await.map(|_| ());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| (1, "download interrupted".into()))?
        {
            if bytes.len() + chunk.len() > todoer::discussion::MAX_ATTACHMENT_BYTES as usize {
                return Err((1, "download exceeds declared limit".into()));
            }
            bytes.extend_from_slice(&chunk);
        }
        let actual_hash = format!("{:x}", Sha256::digest(&bytes));
        if bytes.len() as u64 != expected_size || !actual_hash.eq_ignore_ascii_case(expected_hash) {
            return Err((
                1,
                "download size or SHA256 mismatch; output was not written".into(),
            ));
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .map_err(|_| (2, "output exists or cannot be created".into()))?;
        file.write_all(&bytes)
            .map_err(|_| (1, "could not write verified download".into()))?;
        output_value(
            args,
            &json!({"id":id,"output":output.to_string_lossy(),"size":bytes.len(),"sha256":actual_hash}),
        );
        return Ok(());
    }
    let (bytes, hash) = prepared.expect("upload/resume prepared");
    let plan=match command{
        FileCommand::Upload{task,file,comment,..}=>transfer_json(&client,&args.url,&token,"/agent/activity",json!({"op":"upload_create","task_id":task,"input":{"comment_id":comment,"filename":file.file_name().unwrap_or_default().to_string_lossy(),"size":bytes.len(),"sha256":hash}})).await?,
        FileCommand::Resume{id,..}=>transfer_json(&client,&args.url,&token,"/agent/activity",json!({"op":"upload_get","id":id})).await?,
        _=>unreachable!("download already handled"),
    };
    let intent = &plan["intent"];
    let id = intent["id"]
        .as_str()
        .ok_or((1, "server omitted upload intent ID".into()))?;
    let revision = intent["revision"]
        .as_i64()
        .ok_or((1, "server omitted upload intent revision".into()))?;
    if intent["size"].as_u64() != Some(bytes.len() as u64)
        || !intent["sha256"]
            .as_str()
            .is_some_and(|v| v.eq_ignore_ascii_case(&hash))
    {
        return Err((
            2,
            "selected file does not match upload intent size/SHA256".into(),
        ));
    }
    if intent["finalized"] != true {
        let response = client
            .put(api_url(
                &args.url,
                &format!("/uploads/{}/content?revision={revision}", segment(id)),
            )?)
            .bearer_auth(&token)
            .header("content-type", "application/octet-stream")
            .body(bytes)
            .send()
            .await
            .map_err(|_| (1, format!("upload interrupted; resume intent {id}")))?;
        response_json(response)
            .await
            .map_err(|(code, msg)| (code, format!("{msg}; resume intent {id}")))?;
    }
    let attachment = transfer_json(
        &client,
        &args.url,
        &token,
        "/agent/activity",
        json!({"op":"upload_finalize","id":id,"revision":revision}),
    )
    .await
    .map_err(|(code, msg)| (code, format!("{msg}; resume intent {id}")))?;
    output_value(args, &attachment);
    Ok(())
}

fn format_lists(value: &Value, prefix: char) -> String {
    let rows = value.as_array().expect("list response is an array");
    if rows.is_empty() {
        return "No results.\n".into();
    }
    rows.iter()
        .map(|row| {
            let title = row["title"].as_str().unwrap_or("");
            let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
            let reference = row["public_reference"]
                .as_str()
                .or_else(|| row["reference"].as_str())
                .or_else(|| row["id"].as_str())
                .unwrap_or("?");
            let marker = match row.get("complete").and_then(Value::as_bool) {
                Some(true) => "[x] ",
                Some(false) => "[ ] ",
                None => "",
            };
            let prefix =
                if prefix == '%' && row.get("parent_id").is_some_and(|parent| !parent.is_null()) {
                    '^'
                } else {
                    prefix
                };
            format!("{marker}{title}  {prefix}{reference}\n")
        })
        .collect()
}

fn format_subtree(value: &Value) -> String {
    let rows = value["items"]
        .as_array()
        .expect("subtree items are an array");
    let mut output = String::new();
    for row in rows {
        let depth = row["depth"].as_u64().unwrap_or(0).min(256) as usize;
        output.push_str(&"  ".repeat(depth));
        output.push_str(&format_lists(&json!([row]), '%'));
    }
    if rows.is_empty() {
        output.push_str("No results.\n");
    }
    if let Some(cursor) = value["next_cursor"].as_str() {
        output.push_str(&format!("\nMore results: --cursor {cursor}\n"));
    }
    output
}

fn view_prefix(kind: &str) -> char {
    match kind {
        "space" => '@',
        "list" => '#',
        "section" => '$',
        "task" => '%',
        "subtask" => '^',
        _ => '?',
    }
}

fn view_label(row: &Value) -> String {
    let prefix = view_prefix(row["kind"].as_str().unwrap_or(""));
    let title = row["title"]
        .as_str()
        .unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let marker = match row.get("complete").and_then(Value::as_bool) {
        Some(true) => "[x] ",
        Some(false) => "[ ] ",
        None => "",
    };
    format!("{marker}{prefix}{title}")
}

fn format_view(value: &Value, info: bool) -> String {
    let mut entity = value["entity"].clone();
    entity["kind"] = value["kind"].clone();
    let prefix = view_prefix(value["kind"].as_str().unwrap_or(""));
    let reference = value["reference"].as_str().unwrap_or("?");
    let mut output = format!("{}  {prefix}{reference}\n", view_label(&entity));
    if info
        && let Some(description) = entity["description"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
    {
        output.push_str(&format!("\n{description}\n"));
    }
    if let Some(children) = value["children"].as_array().filter(|rows| !rows.is_empty()) {
        output.push('\n');
        let labels: Vec<_> = children.iter().map(view_label).collect();
        let width = labels.iter().map(|s| s.chars().count()).max().unwrap_or(0);
        for (row, label) in children.iter().zip(labels) {
            let prefix = view_prefix(row["kind"].as_str().unwrap_or(""));
            let reference = row["reference"].as_str().unwrap_or("?");
            output.push_str(&label);
            output.push_str(&" ".repeat(width - label.chars().count() + 2));
            output.push_str(&format!("{prefix}{reference}"));
            if info && row["counts"].is_object() {
                let counts = &row["counts"];
                let mut parts = Vec::new();
                if let Some(sections) = counts["sections"].as_u64() {
                    parts.push(format!("s{sections}"));
                }
                if let Some(open) = counts["open"].as_u64() {
                    parts.push(format!("o{open}"));
                }
                if let Some(done) = counts["done"].as_u64() {
                    parts.push(format!("d{done}"));
                }
                if !parts.is_empty() {
                    output.push_str("  ");
                    output.push_str(&parts.join(" "));
                }
            }
            output.push('\n');
        }
    }
    output
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
        println!("{marker}{id}  {title}");
        if let Some(description) = value
            .get("description")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            println!("    {description}");
        }
    } else {
        let mut readable = value.clone();
        hide_revisions(&mut readable);
        println!(
            "{}",
            serde_json::to_string_pretty(&readable).expect("JSON value serializes")
        );
    }
}

fn hide_revisions(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            fields.retain(|key, _| key != "revision" && !key.ends_with("_revision"));
            for child in fields.values_mut() {
                hide_revisions(child);
            }
        }
        Value::Array(rows) => {
            for row in rows {
                hide_revisions(row);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod list_output_tests {
    use super::*;

    #[test]
    fn lists_are_compact_and_prefer_short_references() {
        let lists = json!([
            {"id":"036cf2d9-5258-4411-89fc-edab0cedf4fe", "reference":"abc2345", "title":"groceries", "revision":1, "description":"Groceries.\n\n[Card](https://example.com)"},
            {"id":"second-id", "title":"travel\nplans", "description":""}
        ]);
        assert_eq!(
            format_lists(&lists, '#'),
            "groceries  #abc2345\ntravel plans  #second-id\n"
        );
        assert_eq!(format_lists(&json!([]), '#'), "No results.\n");
    }

    #[test]
    fn subtree_shows_hierarchy_status_and_continuation() {
        let page = json!({"items":[{"depth":0,"title":"Plan","reference":"abc","parent_id":null,"complete":false},{"depth":1,"title":"Book","reference":"def","parent_id":"root","complete":true}],"next_cursor":"next"});
        assert_eq!(
            format_subtree(&page),
            "[ ] Plan  %abc\n  [x] Book  ^def\n\nMore results: --cursor next\n"
        );
    }

    #[test]
    fn view_routes_prefixed_references_and_rejects_invalid_input() {
        for (reference, expected) in [
            ("@a", "/view/space/a"),
            ("#b", "/view/list/b"),
            ("$c", "/view/section/c"),
            ("%d", "/view/task/d"),
            ("^e", "/view/subtask/e"),
        ] {
            assert_eq!(view_request(reference).unwrap().1, expected);
        }
        assert!(view_request("unknown").is_err());
        assert!(view_request("#").is_err());
        let space = json!({"kind":"space","reference":"k7m2p9c","entity":{"title":"personal","revision":2},"children":[{"kind":"list","title":"groceries","reference":"b7k3m9p","counts":{"sections":1,"open":11,"done":0}}]});
        assert_eq!(
            format_view(&space, false),
            "@personal  @k7m2p9c\n\n#groceries  #b7k3m9p\n"
        );
        assert_eq!(
            format_view(&space, true),
            "@personal  @k7m2p9c\n\n#groceries  #b7k3m9p  s1 o11 d0\n"
        );
        let task = json!({"kind":"task","reference":"abc2345","entity":{"title":"Milk","complete":false,"description":"Buy oat milk.","revision":2},"children":[]});
        assert_eq!(
            format_view(&task, true),
            "[ ] %Milk  %abc2345\n\nBuy oat milk.\n"
        );
        let mut raw = json!({"revision":2,"child":{"revision":3,"title":"Milk"}});
        hide_revisions(&mut raw);
        assert_eq!(raw, json!({"child":{"title":"Milk"}}));
        assert!(Args::try_parse_from(["todoer", "view", "@abc2345", "--info"]).is_ok());
        assert!(Args::try_parse_from(["todoer", "space", "all"]).is_ok());
        assert!(Args::try_parse_from(["todoer", "workspace", "all"]).is_ok());
        assert!(Args::try_parse_from(["todoer", "review", "--space", "abc"]).is_ok());
        assert!(Args::try_parse_from(["todoer", "query", "--space", "abc"]).is_ok());
    }
}
