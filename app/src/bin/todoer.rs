#[path = "cli_auth/mod.rs"]
mod cli_login;
use clap::{Parser, Subcommand};
use reqwest::{Client, Method};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::{IsTerminal, Read, Write};

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
    /// Space administration.
    #[command(name = "spaces", visible_aliases = ["space", "workspace"])]
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
        /// Show a list's sections and top-level tasks.
        #[arg(long)]
        full: bool,
        /// Include nested subtasks in a full list view.
        #[arg(long, requires = "full")]
        sub: bool,
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
        #[arg(long, hide = true)]
        revision: Option<i64>,
        #[arg(long)]
        section: Option<String>,
        #[arg(long)]
        clear: bool,
    },
    #[command(hide = true)]
    Reparent {
        id: String,
        #[arg(long, hide = true, default_value_t = 0)]
        revision: i64,
        #[arg(long)]
        parent: Option<String>,
    },
    Reorder {
        list: String,
        #[arg(long, hide = true, default_value_t = 0)]
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
    /// Create a task using the same capture notation as the app.
    Task {
        #[command(subcommand)]
        command: TaskCommand,
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
    #[command(hide = true)]
    Capture {
        #[arg(long, required_unless_present = "body")]
        list: Option<String>,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        key: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long, conflicts_with = "section")]
        parent: Option<String>,
        #[arg(long)]
        section: Option<String>,
        #[arg(required_unless_present = "body")]
        titles: Vec<String>,
    },
    Edit {
        id: String,
        #[arg(long, hide = true, default_value_t = 0)]
        revision: i64,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        body: Option<String>,
    },
    Complete {
        id: String,
        #[arg(long, hide = true, default_value_t = 0)]
        revision: i64,
        #[arg(long)]
        subtree: bool,
    },
    Reopen {
        id: String,
        #[arg(long, hide = true, default_value_t = 0)]
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
        #[arg(long, visible_alias = "destination")]
        to: String,
        #[arg(long)]
        yes: bool,
    },
    MovePreview {
        id: String,
        #[arg(long)]
        destination: String,
    },
    TrashList,
    Trash {
        id: String,
        #[arg(long, hide = true, default_value_t = 0)]
        revision: i64,
    },
    /// Archive a space or list.
    Archive {
        reference: String,
    },
    /// Restore an archived space or list.
    Restore {
        reference: String,
    },
    Undo {
        seq: i64,
        #[arg(long, hide = true, default_value_t = 0)]
        revision: i64,
    },
    Capabilities,
    Changes {
        #[arg(long, default_value_t = 0)]
        after: i64,
    },
}

#[derive(Subcommand)]
enum TaskCommand {
    Add {
        /// Task title and capture shortcuts, quoted as one argument.
        #[arg(required_unless_present = "body")]
        text: Option<String>,
        /// Full capture request as a JSON object.
        #[arg(long)]
        body: Option<String>,
        /// Reuse a key only when retrying the same capture.
        #[arg(long)]
        key: Option<String>,
    },
}

#[derive(Subcommand)]
enum WorkspaceCommand {
    All {
        #[arg(long, conflicts_with = "archived_only")]
        archived: bool,
        #[arg(long)]
        archived_only: bool,
    },
    Get {
        id: String,
    },
    Create {
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        description: Option<String>,
    },
    Edit {
        id: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        description: Option<String>,
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
    All {
        #[arg(long, conflicts_with = "archived_only")]
        archived: bool,
        #[arg(long)]
        archived_only: bool,
    },
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
        body: Option<String>,
        #[arg(long)]
        space: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        description: Option<String>,
    },
    Edit {
        id: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        description: Option<String>,
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
        body: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        description: Option<String>,
    },
    Edit {
        id: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        description: Option<String>,
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
        #[arg(long, hide = true, default_value_t = 0)]
        revision: i64,
        name: String,
    },
    Get {
        task: String,
    },
    Set {
        task: String,
        #[arg(long, hide = true, default_value_t = 0)]
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
        #[arg(long, hide = true, default_value_t = 0)]
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
        #[arg(long, hide = true, default_value_t = 0)]
        revision: i64,
        #[arg(long)]
        body: String,
    },
    Start {
        id: String,
        #[arg(long, hide = true, default_value_t = 0)]
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
        #[arg(long, hide = true, default_value_t = 0)]
        revision: i64,
        state: String,
    },
    Occurrences {
        id: String,
    },
    Skip {
        id: String,
        #[arg(long, hide = true, default_value_t = 0)]
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
        #[arg(long, hide = true, default_value_t = 0)]
        revision: i64,
        body: String,
    },
    Delete {
        id: String,
        #[arg(long, hide = true, default_value_t = 0)]
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
        #[arg(long, hide = true, default_value_t = 0)]
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
        #[arg(long, hide = true, default_value_t = 0)]
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
fn edit_body(
    body: Option<String>,
    title: Option<String>,
    description: Option<String>,
) -> Result<Value, (i32, String)> {
    let mut value = json!({});
    if let Some(title) = title {
        value["title"] = json!(title);
    }
    if let Some(description) = description {
        value["description"] = json!(description);
    }
    if let Some(body) = body {
        let supplied = object_body(&body)?;
        for (key, field) in supplied.as_object().expect("object_body validates object") {
            value[key] = field.clone();
        }
    }
    if value.as_object().is_none_or(|fields| fields.is_empty()) {
        return Err((2, "provide --body, --title, or --description".into()));
    }
    Ok(value)
}
fn workspace_request(command: WorkspaceCommand) -> Result<RequestSpec, (i32, String)> {
    Ok(match command {
        WorkspaceCommand::All {
            archived,
            archived_only,
        } => read_request(format!(
            "/cli/overview/spaces?archived={archived}&archived_only={archived_only}"
        )),
        WorkspaceCommand::Get { id } => read_request(format!("/workspaces/{}", segment(&id))),
        WorkspaceCommand::Create {
            body,
            title,
            description,
        } => {
            let mut value = edit_body(body, title, description)?;
            if value.get("description").is_none() {
                value["description"] = json!("");
            }
            (Method::POST, "/workspaces".into(), Some(value), None)
        }
        WorkspaceCommand::Edit {
            id,
            body,
            title,
            description,
        } => (
            Method::PATCH,
            format!("/workspaces/{}", segment(&id)),
            Some(edit_body(body, title, description)?),
            None,
        ),
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
        ListCommand::All {
            archived,
            archived_only,
        } => read_request(format!(
            "/cli/overview/lists?archived={archived}&archived_only={archived_only}"
        )),
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
        ListCommand::Create {
            body,
            space,
            title,
            description,
        } => {
            let mut value = edit_body(body, title, description)?;
            if let Some(space) = space.filter(|_| value.get("workspace_id").is_none()) {
                value["workspace_id"] = json!(space);
            }
            (Method::POST, "/lists".into(), Some(value), None)
        }
        ListCommand::Edit {
            id,
            body,
            title,
            description,
        } => (
            Method::PATCH,
            format!("/lists/{}", segment(&id)),
            Some(edit_body(body, title, description)?),
            None,
        ),
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
        SectionCommand::Create {
            list,
            body,
            title,
            description,
        } => (
            Method::POST,
            format!("/lists/{}/sections", segment(&list)),
            Some(edit_body(body, title, description)?),
            None,
        ),
        SectionCommand::Edit {
            id,
            body,
            title,
            description,
        } => (
            Method::PATCH,
            format!("/sections/{}", segment(&id)),
            Some(edit_body(body, title, description)?),
            None,
        ),
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
    if let Command::Move { id, to, yes } = &args.command {
        return move_entity(&args, id, to, *yes).await;
    }
    if let Command::Task {
        command: TaskCommand::Add { text, body, key },
    } = &args.command
    {
        if let Some(body) = body {
            return add_task_json(&args, body, key.as_deref()).await;
        }
        return add_task(
            &args,
            text.as_deref().expect("clap requires task text"),
            key.as_deref(),
        )
        .await;
    }
    if let Command::Archive { reference } | Command::Restore { reference } = &args.command {
        return archive_entity(
            &args,
            reference,
            matches!(&args.command, Command::Archive { .. }),
        )
        .await;
    }
    let listing_prefix = match &args.command {
        Command::List
        | Command::Lists {
            command: ListCommand::All { .. },
        } => Some('#'),
        Command::Workspace {
            command: WorkspaceCommand::All { .. },
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
    let (method, mut path, mut body, key) = match args.command {
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
        Command::Task { .. } => unreachable!("task creation handled before request construction"),
        Command::View {
            reference,
            full,
            sub,
            ..
        } => {
            if full && !reference.starts_with('#') {
                return Err((2, "--full requires a list reference".into()));
            }
            if full {
                query_pairs.push(("full".into(), "true".into()));
                if sub {
                    query_pairs.push(("sub".into(), "true".into()));
                }
            }
            view_request(&reference)?
        }
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
            if section.is_some() || clear {
                action_request(
                    "/organize",
                    json!({"op":"section_set","id":id,"revision":revision.unwrap_or(0),"section_id":section}),
                )
            } else {
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
        Command::List => (Method::GET, "/cli/overview/lists".into(), None, None),
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
        Command::Capture {
            list,
            body,
            key,
            titles,
            description,
            parent,
            section,
        } => {
            let mut value = if let Some(body) = body {
                object_body(&body)?
            } else {
                json!({})
            };
            if value.get("list_id").is_none() {
                if let Some(list) = list {
                    value["list_id"] = json!(unprefix(&list));
                }
            }
            if value.get("section_id").is_none() && section.is_some() {
                value["section_id"] = json!(section);
            }
            if value.get("tasks").is_none() && !titles.is_empty() {
                value["tasks"] = json!(titles.into_iter().map(|title| json!({"title": title, "description": description.as_deref().unwrap_or(""), "parent_id": parent})).collect::<Vec<_>>());
            }
            (
                Method::POST,
                "/capture".into(),
                Some(value),
                Some(key.unwrap_or_else(|| uuid::Uuid::new_v4().to_string())),
            )
        }
        Command::Edit {
            id,
            revision,
            title,
            description,
            body,
        } => {
            let mut body = edit_body(body, title, description)?;
            if body.get("revision").is_none() {
                body["revision"] = json!(revision);
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
        Command::Move { .. } => unreachable!("move handled before request construction"),
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
        Command::Archive { .. } | Command::Restore { .. } => {
            unreachable!("archive handled before request construction")
        }
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
    let token = cli_login::token(&args.url)?;
    normalize_request_path(&client, &args.url, &token, &mut path).await?;
    if path == "/search" || path == "/review" {
        for (key, value) in &mut query_pairs {
            let kind = match key.as_str() {
                "list" => "list",
                "workspace" => "space",
                _ => continue,
            };
            let entity = get_json(
                &client,
                &args.url,
                &token,
                &format!("/view/{kind}/{}", segment(value)),
            )
            .await?;
            *value = entity["entity"]["id"]
                .as_str()
                .ok_or((1, "server omitted entity ID".into()))?
                .to_owned();
        }
    }
    let mut url = reqwest::Url::parse(&format!("{}/api/v1{path}", args.url.trim_end_matches('/')))
        .map_err(|_| (2, "invalid server URL".into()))?;
    if !query_pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(query_pairs);
    }
    if let Some(payload) = body.as_mut() {
        prepare_entity_write(&client, &args.url, &token, &method, &path, payload).await?;
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

async fn normalize_request_path(
    client: &Client,
    base: &str,
    token: &str,
    path: &mut String,
) -> Result<(), (i32, String)> {
    let parts: Vec<_> = path.trim_start_matches('/').split('/').collect();
    if parts.len() < 3 {
        return Ok(());
    }
    let kind = match parts[0] {
        "workspaces" => "space",
        "lists" => "list",
        _ => return Ok(()),
    };
    let view_path = format!("/view/{kind}/{}", parts[1]);
    let response = client
        .get(api_url(base, &view_path)?)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|_| (1, "could not resolve entity reference".into()))?;
    let value = response_json(response).await?.1;
    let id = value["entity"]["id"]
        .as_str()
        .ok_or((1, "server omitted entity ID".into()))?;
    *path = format!("/{}/{}/{}", parts[0], segment(id), parts[2..].join("/"));
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

async fn prepare_entity_write(
    client: &Client,
    base: &str,
    token: &str,
    method: &Method,
    path: &str,
    body: &mut Value,
) -> Result<(), (i32, String)> {
    if path == "/query" {
        for (key, kind) in [
            ("list_id", "list"),
            ("workspace_id", "space"),
            ("section_id", "section"),
        ] {
            if let Some(reference) = body.get(key).and_then(Value::as_str) {
                let entity = get_json(
                    client,
                    base,
                    token,
                    &format!("/view/{kind}/{}", segment(reference)),
                )
                .await?;
                body[key] = entity["entity"]["id"].clone();
            }
        }
    }
    if path == "/capture" {
        if let Some(list) = body.get("list_id").and_then(Value::as_str) {
            body["list_id"] = json!(unprefix(list));
        }
        if let Some(reference) = body.get("section_id").and_then(Value::as_str)
            && reference.starts_with('$')
        {
            let (_, target, _, _) = view_request(reference)?;
            let response = client
                .get(api_url(base, &target)?)
                .bearer_auth(token)
                .send()
                .await
                .map_err(|_| (1, "could not resolve section reference".into()))?;
            body["section_id"] = response_json(response).await?.1["entity"]["id"].clone();
        }
        if let Some(tasks) = body.get_mut("tasks").and_then(Value::as_array_mut) {
            for task in tasks {
                if let Some(reference) = task.get("parent_id").and_then(Value::as_str)
                    && reference.starts_with(['%', '^'])
                {
                    let (_, target, _, _) = view_request(reference)?;
                    let response = client
                        .get(api_url(base, &target)?)
                        .bearer_auth(token)
                        .send()
                        .await
                        .map_err(|_| (1, "could not resolve parent reference".into()))?;
                    task["parent_id"] = response_json(response).await?.1["entity"]["id"].clone();
                }
            }
        }
    }
    if path == "/bulk/preview" {
        if let Some(operations) = body["operations"].as_array_mut() {
            for operation in operations {
                for key in ["id", "destination", "section_id", "parent_id"] {
                    if let Some(reference) = operation.get(key).and_then(Value::as_str)
                        && reference.starts_with(['@', '#', '$', '%', '^'])
                    {
                        let (_, target, _, _) = view_request(reference)?;
                        let response = client
                            .get(api_url(base, &target)?)
                            .bearer_auth(token)
                            .send()
                            .await
                            .map_err(|_| (1, "could not resolve entity reference".into()))?;
                        let value = response_json(response).await?.1;
                        operation[key] = value["entity"]["id"].clone();
                    }
                }
                if operation
                    .get("revision")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    == 0
                {
                    let id = operation["id"]
                        .as_str()
                        .ok_or((2, "bulk operation requires an ID".into()))?;
                    let response = client
                        .get(api_url(base, &format!("/tasks/{}", segment(id)))?)
                        .bearer_auth(token)
                        .send()
                        .await
                        .map_err(|_| (1, "could not read current task".into()))?;
                    let value = response_json(response).await?.1;
                    operation["revision"] = value["revision"].clone();
                }
            }
        }
    }
    if path.ends_with("/reorder")
        || matches!(
            path,
            "/organize"
                | "/agent/schedule"
                | "/agent/activity"
                | "/references"
                | "/destinations/preferences"
        )
    {
        for key in [
            "id",
            "list_id",
            "workspace_id",
            "task_id",
            "section_id",
            "parent_id",
            "source_id",
            "target_id",
        ] {
            if let Some(reference) = body.get(key).and_then(Value::as_str)
                && reference.starts_with(['@', '#', '$', '%', '^'])
            {
                let (_, target, _, _) = view_request(reference)?;
                let response = client
                    .get(api_url(base, &target)?)
                    .bearer_auth(token)
                    .send()
                    .await
                    .map_err(|_| (1, "could not resolve entity reference".into()))?;
                let value = response_json(response).await?.1;
                let id = value["entity"]["id"]
                    .as_str()
                    .ok_or((1, "server omitted entity ID".into()))?;
                body[key] = json!(id);
            }
        }
        if let Some(ids) = body.get_mut("ids").and_then(Value::as_array_mut) {
            for item in ids {
                if let Some(reference) = item.as_str()
                    && reference.starts_with(['@', '#', '$', '%', '^'])
                {
                    let (_, target, _, _) = view_request(reference)?;
                    let response = client
                        .get(api_url(base, &target)?)
                        .bearer_auth(token)
                        .send()
                        .await
                        .map_err(|_| (1, "could not resolve entity reference".into()))?;
                    let value = response_json(response).await?.1;
                    *item = value["entity"]["id"].clone();
                }
            }
        }
    }
    if *method == Method::POST && path == "/lists" {
        if let Some(space) = body["workspace_id"].as_str() {
            let url = api_url(base, &format!("/view/space/{}", segment(space)))?;
            let response = client
                .get(url)
                .bearer_auth(token)
                .send()
                .await
                .map_err(|_| (1, "could not resolve space reference".into()))?;
            let value = response_json(response).await?.1;
            body["workspace_id"] = value["entity"]["id"].clone();
        }
    }
    let parts: Vec<_> = path.trim_start_matches('/').split('/').collect();
    let needs_revision = body.get("revision").and_then(Value::as_i64).unwrap_or(0) == 0;
    if needs_revision {
        let op = body["op"].as_str().unwrap_or("");
        let action_id = body["id"].as_str().unwrap_or("");
        let fetch = match (method, parts.as_slice()) {
            (&Method::PATCH, ["tasks", id]) => Some(format!("/tasks/{id}")),
            (&Method::POST, ["tasks", id, "trash"]) => Some(format!("/tasks/{id}")),
            (&Method::PATCH, ["workspaces", id]) => Some(format!("/view/space/{id}")),
            (&Method::PATCH, ["lists", id]) => Some(format!("/view/list/{id}")),
            (&Method::PATCH, ["sections", id]) => Some(format!("/view/section/{id}")),
            (&Method::POST, ["workspaces", id, "members", ..]) => Some(format!("/view/space/{id}")),
            (&Method::POST, ["workspaces", id, "lists", "reorder"]) => {
                Some(format!("/view/space/{id}"))
            }
            (&Method::POST, ["lists", id, "access" | "grants", ..]) => {
                Some(format!("/view/list/{id}"))
            }
            (&Method::POST, ["lists", id, "sections", "reorder"]) => {
                Some(format!("/view/list/{id}"))
            }
            (&Method::POST, ["organize"])
                if matches!(op, "tags_set" | "section_set" | "reparent") =>
            {
                Some(format!("/tasks/{}", segment(action_id)))
            }
            (&Method::POST, ["organize"]) if op == "reorder" => Some(format!(
                "/view/list/{}",
                segment(body["list_id"].as_str().unwrap_or(""))
            )),
            (&Method::POST, ["organize"]) if op == "tag_rename" => {
                Some(format!("/cli/tags/{}", segment(action_id)))
            }
            (&Method::POST, ["agent", "schedule"])
                if matches!(op, "set" | "series_start" | "skip") =>
            {
                Some(format!("/tasks/{}", segment(action_id)))
            }
            (&Method::POST, ["references"]) if op == "bind" && body["source_kind"] == "task" => {
                Some(format!(
                    "/tasks/{}",
                    segment(body["source_id"].as_str().unwrap_or(""))
                ))
            }
            _ => None,
        };
        if let Some(fetch) = fetch {
            let response = client
                .get(api_url(base, &fetch)?)
                .bearer_auth(token)
                .send()
                .await
                .map_err(|_| (1, "could not read current entity".into()))?;
            let value = response_json(response).await?.1;
            let revision = value
                .get("revision")
                .or_else(|| value.pointer("/entity/revision"))
                .and_then(Value::as_i64)
                .ok_or((1, "server omitted current version".into()))?;
            body["revision"] = json!(revision);
        } else if *method == Method::POST {
            let read_action = match (path, op) {
                ("/agent/schedule", "series_state") => {
                    Some(json!({"op":"series_get","id":action_id}))
                }
                ("/agent/activity", "comment_edit" | "comment_delete") => {
                    Some(json!({"op":"comment_get","id":action_id}))
                }
                ("/agent/activity", "file_delete") => {
                    Some(json!({"op":"file_link","id":action_id}))
                }
                ("/agent/activity", "upload_finalize") => {
                    Some(json!({"op":"upload_get","id":action_id}))
                }
                ("/references", "bind") if body["source_kind"] == "comment" => {
                    Some(json!({"op":"comment_get","id":body["source_id"]}))
                }
                _ => None,
            };
            if let Some(read_action) = read_action {
                let read_path = if path == "/references" {
                    "/agent/activity"
                } else {
                    path
                };
                let value = transfer_json(client, base, token, read_path, read_action).await?;
                let revision = value
                    .get("revision")
                    .or_else(|| value.pointer("/attachment/revision"))
                    .or_else(|| value.pointer("/intent/revision"))
                    .and_then(Value::as_i64)
                    .ok_or((1, "server omitted current version".into()))?;
                body["revision"] = json!(revision);
            } else if let ["history", seq, "undo"] = parts.as_slice() {
                let seq: i64 = seq
                    .parse()
                    .map_err(|_| (2, "invalid history sequence".into()))?;
                let response = client
                    .get(api_url(
                        base,
                        &format!("/changes?after={}", seq.saturating_sub(1)),
                    )?)
                    .bearer_auth(token)
                    .send()
                    .await
                    .map_err(|_| (1, "could not read history".into()))?;
                let changes = response_json(response).await?.1;
                let revision = changes
                    .as_array()
                    .and_then(|rows| rows.iter().find(|row| row["seq"].as_i64() == Some(seq)))
                    .and_then(|row| row.pointer("/task/revision"))
                    .and_then(Value::as_i64)
                    .ok_or((1, "could not find this history entry".into()))?;
                body["revision"] = json!(revision);
            }
        }
    }
    Ok(())
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

fn parse_app_capture(
    text: &str,
    destinations: &Value,
    sections: &Value,
    today: &str,
) -> Result<Value, (i32, String)> {
    let dates =
        include_str!("../../assets/capture-dates.mjs").replace("export function ", "function ");
    let parser = include_str!("../../assets/smart-capture-parser.mjs")
        .replace(
            "import { captureDateSpans } from './capture-dates.mjs';",
            "",
        )
        .replace("export function ", "function ");
    let date = chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d")
        .map_err(|_| (1, "invalid capture date".into()))?;
    use chrono::Datelike;
    let source = format!(
        "{dates}\n{parser}\nJSON.stringify(parseCapture({}, {{destinations:{destinations},sections:{sections},now:new Date({},{},{},12),dates:true}}))",
        json!(text),
        date.year(),
        date.month0(),
        date.day()
    );
    let runtime =
        rquickjs::Runtime::new().map_err(|_| (1, "could not initialize capture parser".into()))?;
    let context = rquickjs::Context::full(&runtime)
        .map_err(|_| (1, "could not initialize capture parser".into()))?;
    let parsed: String = context
        .with(|ctx| ctx.eval(source))
        .map_err(|err| (1, format!("capture parser failed: {err}")))?;
    serde_json::from_str(&parsed).map_err(|_| (1, "capture parser returned invalid output".into()))
}

async fn get_json(
    client: &Client,
    base: &str,
    token: &str,
    path: &str,
) -> Result<Value, (i32, String)> {
    let response = client
        .get(api_url(base, path)?)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|_| (1, "request failed; check server connectivity".into()))?;
    Ok(response_json(response).await?.1)
}

async fn add_task_json(args: &Args, input: &str, key: Option<&str>) -> Result<(), (i32, String)> {
    let mut body = object_body(input)?;
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|_| (1, "could not initialize HTTP client".into()))?;
    let token = cli_login::token(&args.url)?;
    if body.get("list_id").is_none() {
        body["list_id"] = get_json(&client, &args.url, &token, "/inbox").await?["list_id"].clone();
    }
    if body.get("tasks").is_none() {
        let fields = body.as_object_mut().expect("object_body validates object");
        let title = fields
            .remove("title")
            .ok_or((2, "JSON requires title or tasks".into()))?;
        let description = fields.remove("description").unwrap_or(json!(""));
        let parent = fields.remove("parent_id").unwrap_or(Value::Null);
        body["tasks"] = json!([{"title":title,"description":description,"parent_id":parent}]);
    }
    if body["tasks"]
        .as_array()
        .is_none_or(|tasks| tasks.len() != 1)
    {
        return Err((2, "task add requires exactly one task".into()));
    }
    prepare_entity_write(
        &client,
        &args.url,
        &token,
        &Method::POST,
        "/capture",
        &mut body,
    )
    .await?;
    let generated_key = uuid::Uuid::new_v4().to_string();
    let response = client
        .post(api_url(&args.url, "/capture")?)
        .bearer_auth(&token)
        .header("Idempotency-Key", key.unwrap_or(&generated_key))
        .json(&body)
        .send()
        .await
        .map_err(|_| (1, "capture request failed".into()))?;
    let tasks = response_json(response).await?.1;
    let task = tasks
        .as_array()
        .and_then(|tasks| tasks.first())
        .ok_or((1, "server omitted created task".into()))?;
    let list_id = task["list_id"]
        .as_str()
        .ok_or((1, "server omitted task list".into()))?;
    let list = get_json(
        &client,
        &args.url,
        &token,
        &format!("/view/list/{}?full=true&sub=true", segment(list_id)),
    )
    .await
    .map_err(|(code, message)| {
        (
            code,
            format!("Task was created, but its list could not be loaded: {message}"),
        )
    })?;
    if args.json {
        println!("{}", json!({"task":task,"list":list}));
    } else {
        println!(
            "Added task '{}' to list '#{}'.\n",
            task["title"].as_str().unwrap_or(""),
            list["entity"]["title"].as_str().unwrap_or("?")
        );
        print!("{}", format_full_view(&list, false));
    }
    Ok(())
}

async fn add_task(args: &Args, text: &str, key: Option<&str>) -> Result<(), (i32, String)> {
    if text.trim().is_empty() {
        return Err((2, "task text cannot be empty".into()));
    }
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|_| (1, "could not initialize HTTP client".into()))?;
    let token = cli_login::token(&args.url)?;
    let options = get_json(&client, &args.url, &token, "/capture/options").await?;
    let lists = get_json(&client, &args.url, &token, "/cli/overview/lists").await?;
    let spaces = get_json(&client, &args.url, &token, "/cli/overview/spaces").await?;
    let mut destinations = options["destinations"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for list in lists.as_array().into_iter().flatten() {
        if !destinations.iter().any(|d| d["list_id"] == list["id"]) {
            destinations.push(json!({"space":"","space_id":"","list":list["title"],"list_id":list["id"],"default_list":false}));
        }
    }
    for destination in &mut destinations {
        if let Some(list) = lists
            .as_array()
            .into_iter()
            .flatten()
            .find(|row| row["id"] == destination["list_id"])
            && let Some(reference) = list["public_reference"].as_str()
            && text.contains(&format!("#{reference}"))
        {
            destination["list"] = json!(reference);
        }
        if let Some(space) = spaces
            .as_array()
            .into_iter()
            .flatten()
            .find(|row| row["id"] == destination["space_id"])
            && let Some(reference) = space["public_reference"].as_str()
            && text.contains(&format!("@{reference}"))
        {
            destination["space"] = json!(reference);
        }
    }
    let timezone: chrono_tz::Tz = options["timezone"]
        .as_str()
        .unwrap_or("UTC")
        .parse()
        .unwrap_or(chrono_tz::UTC);
    let today = chrono::Utc::now()
        .with_timezone(&timezone)
        .date_naive()
        .to_string();
    let destinations = json!(destinations);
    let preliminary = parse_app_capture(text, &destinations, &json!([]), &today)?;
    if let Some(warnings) = preliminary["warnings"]
        .as_array()
        .filter(|rows| !rows.is_empty())
    {
        return Err((
            2,
            warnings
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    let list_id = if let Some(id) = preliminary
        .pointer("/destination/list_id")
        .and_then(Value::as_str)
    {
        id.to_owned()
    } else {
        get_json(&client, &args.url, &token, "/inbox").await?["list_id"]
            .as_str()
            .ok_or((1, "server omitted inbox".into()))?
            .to_owned()
    };
    let mut sections = if list_id.starts_with("intake:") {
        vec![]
    } else {
        get_json(
            &client,
            &args.url,
            &token,
            &format!("/lists/{}/sections", segment(&list_id)),
        )
        .await?
        .as_array()
        .cloned()
        .unwrap_or_default()
    };
    for section in &mut sections {
        if let Some(reference) = section["reference"].as_str()
            && text.contains(&format!("${reference}"))
        {
            section["title"] = json!(reference);
        }
    }
    let parsed = parse_app_capture(text, &destinations, &json!(sections), &today)?;
    if let Some(warnings) = parsed["warnings"]
        .as_array()
        .filter(|rows| !rows.is_empty())
    {
        return Err((
            2,
            warnings
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    let title = parsed["title"]
        .as_str()
        .filter(|title| !title.trim().is_empty())
        .ok_or((2, "task title cannot be empty".into()))?;
    let body = json!({"list_id":list_id,"section_id":parsed.get("section_id"),"tasks":[{"title":title,"description":"","parent_id":null}],"metadata":{"priority":parsed["priority"],"due_date":parsed["due"],"planned_date":parsed["planned"],"duration_minutes":parsed["duration"],"tags":parsed["tags"]}});
    let generated_key = uuid::Uuid::new_v4().to_string();
    let response = client
        .post(api_url(&args.url, "/capture")?)
        .bearer_auth(&token)
        .header("Idempotency-Key", key.unwrap_or(&generated_key))
        .json(&body)
        .send()
        .await
        .map_err(|_| (1, "capture request failed".into()))?;
    let tasks = response_json(response).await?.1;
    let task = tasks
        .as_array()
        .and_then(|rows| rows.first())
        .ok_or((1, "server omitted created task".into()))?;
    let actual_list = task["list_id"]
        .as_str()
        .ok_or((1, "server omitted task list".into()))?;
    let list = get_json(
        &client,
        &args.url,
        &token,
        &format!("/view/list/{}?full=true&sub=true", segment(actual_list)),
    )
    .await
    .map_err(|(code, message)| {
        (
            code,
            format!("Task was created, but its list could not be loaded: {message}"),
        )
    })?;
    if args.json {
        println!("{}", json!({"task":task,"list":list,"capture":parsed}));
    } else {
        let priority = parsed["priority"]
            .as_u64()
            .map(|p| format!("P{p} "))
            .unwrap_or_default();
        let planned = parsed["planned"]
            .as_str()
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .map(|d| format!(" planned for {}", d.format("%b %-d")))
            .unwrap_or_default();
        let due = parsed["due"]
            .as_str()
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .map(|d| format!(" due {}", d.format("%b %-d")))
            .unwrap_or_default();
        let duration = parsed["duration"]
            .as_u64()
            .map(|n| {
                if n % 60 == 0 {
                    format!(" ({}h)", n / 60)
                } else {
                    format!(" ({n}m)")
                }
            })
            .unwrap_or_default();
        let tags = parsed["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(|s| format!(" +{s}"))
            .collect::<String>();
        println!(
            "Added {priority}task '{title}' to list '#{}'{planned}{due}{duration}{tags}.\n",
            list["entity"]["title"].as_str().unwrap_or("?")
        );
        print!("{}", format_full_view(&list, false));
    }
    Ok(())
}

async fn move_entity(
    args: &Args,
    source: &str,
    destination: &str,
    yes: bool,
) -> Result<(), (i32, String)> {
    let source_type = source.chars().next().unwrap_or('?');
    let destination_type = destination.chars().next().unwrap_or('?');
    if !matches!(
        (source_type, destination_type),
        ('#', '@') | ('$', '#') | ('%' | '^', '#' | '$' | '%' | '^')
    ) {
        return Err((2, "unsupported move: use a list to space, section to list, or task to list, section, or task".into()));
    }
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|_| (1, "could not initialize HTTP client".into()))?;
    let token = cli_login::token(&args.url)?;
    let read = async |reference: &str| -> Result<Value, (i32, String)> {
        let (_, path, _, _) = view_request(reference)?;
        let response = client
            .get(api_url(&args.url, &path)?)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|_| (1, "could not read move destination".into()))?;
        Ok(response_json(response).await?.1)
    };
    let from = read(source).await?;
    let to = read(destination).await?;
    let source_id = from["entity"]["id"]
        .as_str()
        .ok_or((1, "server omitted source ID".into()))?;
    let dest_id = to["entity"]["id"]
        .as_str()
        .ok_or((1, "server omitted destination ID".into()))?;
    let revision = from["entity"]["revision"]
        .as_i64()
        .ok_or((1, "server omitted current version".into()))?;
    let (apply_path, apply_body, broadens) = match (source_type, destination_type) {
        ('#', '@') | ('$', '#') => {
            let (prefix, field) = if source_type == '#' {
                ("lists", "destination_workspace_id")
            } else {
                ("sections", "destination_list_id")
            };
            let path = if source_type == '#' {
                format!("/{prefix}/{}/workspace-move", segment(source_id))
            } else {
                format!("/{prefix}/{}/move", segment(source_id))
            };
            let mut input = json!({});
            input[field] = json!(dest_id);
            let preview = transfer_json(
                &client,
                &args.url,
                &token,
                &format!("{path}/preview"),
                input,
            )
            .await?;
            let mut payload = json!({});
            let fields: &[&str] = if source_type == '#' {
                &[
                    "destination_workspace_id",
                    "list_revision",
                    "source_workspace_revision",
                    "destination_workspace_revision",
                    "policy_hash",
                ]
            } else {
                &[
                    "destination_list_id",
                    "section_revision",
                    "source_list_revision",
                    "destination_list_revision",
                    "policy_hash",
                ]
            };
            for field in fields {
                payload[*field] = preview[*field].clone();
            }
            payload["acknowledge_access_change"] = json!(true);
            (path, payload, preview["audience_broadens"] == true)
        }
        _ => {
            let operation = match destination_type {
                '#' => {
                    json!({"op":"move","id":source_id,"revision":revision,"destination":dest_id,"acknowledge_access_change":true})
                }
                '$' => {
                    json!({"op":"section","id":source_id,"revision":revision,"section_id":dest_id})
                }
                '%' | '^' => {
                    json!({"op":"reparent","id":source_id,"revision":revision,"parent_id":dest_id})
                }
                _ => unreachable!(),
            };
            let preview = transfer_json(
                &client,
                &args.url,
                &token,
                "/bulk/preview",
                json!({"operations":[operation]}),
            )
            .await?;
            let broadens =
                preview.pointer("/steps/0/access_change/audience_broadens") == Some(&json!(true));
            (
                "/bulk/apply".into(),
                json!({"preview_id":preview["preview_id"],"expected_hash":preview["expected_hash"],"idempotency_key":uuid::Uuid::new_v4().to_string()}),
                broadens,
            )
        }
    };
    if broadens {
        eprintln!("This move gives additional people access to the moved content.");
    }
    if !yes {
        if !std::io::stdin().is_terminal() {
            return Err((
                2,
                "confirmation requires an interactive terminal; use --yes to confirm".into(),
            ));
        }
        eprint!(
            "Are you sure to move {} ({source}) to {} ({destination})? (Y/n) ",
            from["entity"]["title"].as_str().unwrap_or(source),
            to["entity"]["title"].as_str().unwrap_or(destination)
        );
        std::io::stderr()
            .flush()
            .map_err(|_| (1, "could not display confirmation".into()))?;
        let mut answer = String::new();
        std::io::stdin()
            .read_line(&mut answer)
            .map_err(|_| (1, "could not read confirmation".into()))?;
        if !matches!(
            answer.trim().to_ascii_lowercase().as_str(),
            "" | "y" | "yes"
        ) {
            return Err((2, "move cancelled".into()));
        }
    }
    let result = transfer_json(&client, &args.url, &token, &apply_path, apply_body).await?;
    if args.json {
        output_value(args, &result);
    } else {
        println!("Moved {source} to {destination}.");
    }
    Ok(())
}

async fn archive_entity(args: &Args, reference: &str, archive: bool) -> Result<(), (i32, String)> {
    let kind = reference.chars().next().unwrap_or('?');
    if !matches!(kind, '@' | '#') && (archive || !matches!(kind, '%' | '^')) {
        return Err((
            2,
            "archiving is currently supported for spaces and lists".into(),
        ));
    }
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|_| (1, "could not initialize HTTP client".into()))?;
    let token = cli_login::token(&args.url)?;
    if !archive && matches!(kind, '%' | '^') {
        let trash = get_json(&client, &args.url, &token, "/trash").await?;
        let row = trash
            .as_array()
            .into_iter()
            .flatten()
            .find(|row| {
                (row["public_reference"].as_str() == Some(unprefix(reference))
                    || row["id"].as_str() == Some(unprefix(reference)))
                    && (kind == '^') == !row["parent_id"].is_null()
            })
            .ok_or((2, "task was not found in trash".into()))?;
        let id = row["id"]
            .as_str()
            .ok_or((1, "server omitted task ID".into()))?;
        let revision = row["revision"]
            .as_i64()
            .ok_or((1, "server omitted current version".into()))?;
        let value = transfer_json(
            &client,
            &args.url,
            &token,
            &format!("/tasks/{}/restore", segment(id)),
            json!({"revision":revision}),
        )
        .await?;
        if args.json {
            output_value(args, &value);
        } else {
            println!("Restored {reference}.");
        }
        return Ok(());
    }
    let (_, path, _, _) = view_request(reference)?;
    let response = client
        .get(api_url(&args.url, &path)?)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|_| (1, "could not read entity".into()))?;
    let entity = response_json(response).await?.1;
    let id = entity["entity"]["id"]
        .as_str()
        .ok_or((1, "server omitted entity ID".into()))?;
    let revision = entity["entity"]["revision"]
        .as_i64()
        .ok_or((1, "server omitted current version".into()))?;
    let (method, path, body) = if kind == '@' {
        (
            Method::POST,
            format!("/workspaces/{}/lifecycle", segment(id)),
            json!({"revision":revision,"action":if archive { "archive" } else { "restore" },"confirmation":null}),
        )
    } else {
        (
            Method::PATCH,
            format!("/lists/{}", segment(id)),
            json!({"revision":revision,"archived":archive}),
        )
    };
    let response = client
        .request(method, api_url(&args.url, &path)?)
        .bearer_auth(&token)
        .json(&body)
        .send()
        .await
        .map_err(|_| (1, "archive request failed".into()))?;
    let value = response_json(response).await?.1;
    if args.json {
        output_value(args, &value);
    } else {
        println!(
            "{} {reference}.",
            if archive { "Archived" } else { "Restored" }
        );
    }
    Ok(())
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
            let archived = if row["archived"] == true {
                "  [archived]"
            } else {
                ""
            };
            format!("{marker}{title}  {prefix}{reference}{archived}\n")
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

fn count_text(counts: &Value) -> String {
    [("sections", "s"), ("open", "o"), ("done", "d")]
        .into_iter()
        .filter_map(|(field, prefix)| counts[field].as_u64().map(|n| format!("{prefix}{n}")))
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_full_view(value: &Value, info: bool) -> String {
    let mut entity = value["entity"].clone();
    entity["kind"] = json!("list");
    let mut output = format!(
        "{}  #{}",
        view_label(&entity),
        value["reference"].as_str().unwrap_or("?")
    );
    if info {
        let counts = count_text(&value["counts"]);
        if !counts.is_empty() {
            output.push_str(&format!("  {counts}"));
        }
    }
    output.push('\n');
    if let Some(description) = entity["description"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
    {
        output.push_str(&format!("\n{description}\n"));
    }
    if let Some(children) = value["children"].as_array() {
        let mut unsectioned = false;
        for row in children {
            if row["kind"] == "section" {
                output.push_str(&format!(
                    "\n{}  ${}",
                    view_label(row),
                    row["reference"].as_str().unwrap_or("?")
                ));
                if info {
                    let counts = count_text(&row["counts"]);
                    if !counts.is_empty() {
                        output.push_str(&format!("  {counts}"));
                    }
                }
                output.push('\n');
                if let Some(tasks) = row["children"].as_array() {
                    for task in tasks {
                        append_full_task(&mut output, task, 1);
                    }
                }
            } else {
                if !unsectioned {
                    output.push('\n');
                    unsectioned = true;
                }
                append_full_task(&mut output, row, 0);
            }
        }
    }
    output
}

fn append_full_task(output: &mut String, task: &Value, indent: usize) {
    let depth = task["depth"].as_u64().unwrap_or(0).min(256) as usize;
    let prefix = view_prefix(task["kind"].as_str().unwrap_or(""));
    output.push_str(&format!(
        "{}{}  {prefix}{}\n",
        "  ".repeat(indent + depth),
        view_label(task),
        task["reference"].as_str().unwrap_or("?")
    ));
}

fn format_view(value: &Value, info: bool) -> String {
    if value["full"] == true {
        return format_full_view(value, info);
    }
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
                let counts = count_text(&row["counts"]);
                if !counts.is_empty() {
                    output.push_str("  ");
                    output.push_str(&counts);
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
        assert!(
            Args::try_parse_from(["todoer", "view", "#abc2345", "--full", "--sub", "--info"])
                .is_ok()
        );
        assert!(Args::try_parse_from(["todoer", "view", "#abc2345", "--sub"]).is_err());
        assert!(Args::try_parse_from(["todoer", "spaces", "all"]).is_ok());
        assert!(Args::try_parse_from(["todoer", "space", "all"]).is_ok());
        assert!(Args::try_parse_from(["todoer", "workspace", "all"]).is_ok());
        assert!(Args::try_parse_from(["todoer", "review", "--space", "abc"]).is_ok());
        assert!(Args::try_parse_from(["todoer", "query", "--space", "abc"]).is_ok());
    }

    #[test]
    fn full_list_output_shows_sections_and_nested_tasks_with_optional_counts() {
        let value = json!({"full":true,"kind":"list","reference":"listref","entity":{"title":"groceries","description":"List for groceries."},"counts":{"sections":1,"open":2,"done":1},"children":[{"kind":"section","title":"veggies","reference":"sectionref","counts":{"open":1,"done":1},"children":[{"kind":"task","title":"Tomatoes","reference":"taskref","complete":false,"depth":0},{"kind":"subtask","title":"Check ripeness","reference":"subref","complete":false,"depth":1},{"kind":"task","title":"Carrots","reference":"done","complete":true,"depth":0}]},{"kind":"task","title":"Milk","reference":"milk","complete":false,"depth":0}]});
        assert_eq!(
            format_view(&value, true),
            "#groceries  #listref  s1 o2 d1\n\nList for groceries.\n\n$veggies  $sectionref  o1 d1\n  [ ] %Tomatoes  %taskref\n    [ ] ^Check ripeness  ^subref\n  [x] %Carrots  %done\n\n[ ] %Milk  %milk\n"
        );
        assert!(!format_view(&value, false).contains("s1 o2 d1"));
    }

    #[test]
    fn entity_edit_flags_and_json_can_be_used_without_versions() {
        for args in [
            vec!["todoer", "spaces", "create", "--title", "Personal"],
            vec![
                "todoer", "lists", "create", "--space", "@abc", "--title", "Shopping",
            ],
            vec!["todoer", "sections", "edit", "$abc", "--title", "Produce"],
            vec!["todoer", "edit", "%abc", "--title", "Milk"],
            vec!["todoer", "complete", "%abc"],
        ] {
            assert!(Args::try_parse_from(args).is_ok());
        }
        assert_eq!(
            edit_body(
                Some("{\"title\":\"JSON\"}".into()),
                Some("flag".into()),
                None
            )
            .unwrap(),
            json!({"title":"JSON"})
        );
    }

    #[test]
    fn task_add_uses_the_app_capture_parser() {
        let options = json!([{"space":"Personal","space_id":"s1","list":"abc123d","list_id":"l1","default_list":true}]);
        let parsed = parse_app_capture(
            "buy milk #abc123d .tomo :1h +family !!",
            &options,
            &json!([]),
            "2026-11-29",
        )
        .unwrap();
        assert_eq!(parsed["title"], "buy milk");
        assert_eq!(parsed["destination"]["list_id"], "l1");
        assert_eq!(parsed["priority"], 1);
        assert_eq!(parsed["planned"], "2026-11-30");
        assert_eq!(parsed["duration"], 60);
        assert_eq!(parsed["tags"], json!(["family"]));
    }
}
