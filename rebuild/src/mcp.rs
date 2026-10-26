//! MCP transport with live per-request bearer authorization.
//! The caller also applies transport authentication before merging this router.
use crate::{
    core::{Capture, Change, Error, NewTask},
    web::{ApiError, AppState},
};
use rmcp::{
    RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, ServerCapabilities, ServerConfig},
    schemars::{self, JsonSchema},
    service::RequestContext,
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone)]
struct TodoerMcp {
    state: AppState,
    tool_router: ToolRouter<Self>,
}

pub fn router(state: AppState) -> axum::Router {
    let service: StreamableHttpService<TodoerMcp, LocalSessionManager> = StreamableHttpService::new(
        move || {
            Ok(TodoerMcp {
                state: state.clone(),
                tool_router: TodoerMcp::tool_router(),
            })
        },
        Default::default(),
        StreamableHttpServerConfig::default(),
    );
    axum::Router::new().nest_service("/mcp", service)
}

fn result<T: Serialize>(result: Result<T, ApiError>) -> CallToolResult {
    match result {
        Ok(value) => CallToolResult::success(vec![ContentBlock::text(
            serde_json::to_string(&value).expect("serializable tool response"),
        )]),
        Err(error) => CallToolResult::error(vec![ContentBlock::text(
            json!({"error":{"code":error.1,"message":error.2}}).to_string(),
        )]),
    }
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Destination {
    destination: Option<String>,
    preference_key: Option<String>,
    #[serde(default)]
    fallback_inbox: bool,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DestinationPreferenceInput {
    key: Option<String>,
    list_id: Option<String>,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PreviewChangesInput {
    #[schemars(schema_with = "bulk_operations_schema")]
    operations: Vec<serde_json::Value>,
}
fn bulk_operations_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let document: serde_json::Value =
        serde_json::from_str(include_str!("../openapi.json")).expect("valid checked-in OpenAPI");
    schemars::Schema::try_from(json!({"type":"array","minItems":1,"maxItems":50,"items":document["components"]["schemas"]["BulkOperation"].clone()})).expect("bulk schema is an object")
}
fn api_schema(name: &str) -> schemars::Schema {
    let document: serde_json::Value =
        serde_json::from_str(include_str!("../openapi.json")).expect("valid checked-in OpenAPI");
    fn inline(
        value: &serde_json::Value,
        document: &serde_json::Value,
        depth: usize,
    ) -> serde_json::Value {
        assert!(depth < 32, "acyclic bounded API schemas");
        match value {
            serde_json::Value::Object(map) => {
                if let Some(reference) = map.get("$ref").and_then(|v| v.as_str()) {
                    return inline(
                        document
                            .pointer(reference.strip_prefix('#').expect("local schema reference"))
                            .expect("existing schema reference"),
                        document,
                        depth + 1,
                    );
                }
                serde_json::Value::Object(
                    map.iter()
                        .map(|(k, v)| (k.clone(), inline(v, document, depth + 1)))
                        .collect(),
                )
            }
            serde_json::Value::Array(items) => serde_json::Value::Array(
                items
                    .iter()
                    .map(|v| inline(v, document, depth + 1))
                    .collect(),
            ),
            _ => value.clone(),
        }
    }
    schemars::Schema::try_from(inline(
        &document["components"]["schemas"][name],
        &document,
        0,
    ))
    .expect("valid API schema")
}
fn query_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    api_schema("StructuredQuery")
}
fn organize_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    api_schema("OrganizeAction")
}
fn reference_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    api_schema("ReferenceAction")
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct QueryInput {
    #[schemars(schema_with = "query_schema")]
    query: serde_json::Value,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OrganizeInput {
    #[schemars(schema_with = "organize_schema")]
    action: serde_json::Value,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReferenceInput {
    #[schemars(schema_with = "reference_schema")]
    action: serde_json::Value,
}
fn schedule_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    api_schema("ScheduleAction")
}
fn activity_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    api_schema("ActivityAction")
}
fn access_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    api_schema("AccessAction")
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ScheduleToolInput {
    #[schemars(schema_with = "schedule_schema")]
    action: serde_json::Value,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ActivityToolInput {
    #[schemars(schema_with = "activity_schema")]
    action: serde_json::Value,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AccessToolInput {
    #[schemars(schema_with = "access_schema")]
    action: serde_json::Value,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ApplyChangesInput {
    preview_id: String,
    expected_hash: String,
    idempotency_key: String,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TaskId {
    id: String,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CaptureInput {
    list_id: String,
    idempotency_key: String,
    tasks: Vec<TaskInput>,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TaskInput {
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    parent_id: Option<String>,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct UpdateInput {
    id: String,
    revision: i64,
    title: Option<String>,
    description: Option<String>,
    complete: Option<bool>,
    #[serde(default)]
    complete_subtree: bool,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MovePreviewInput {
    id: String,
    destination: String,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MoveInput {
    id: String,
    destination: String,
    revision: i64,
    #[serde(default)]
    acknowledge_access_change: bool,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SearchInput {
    #[serde(default)]
    query: String,
    list: Option<String>,
    complete: Option<bool>,
    #[serde(default = "default_limit")]
    limit: i64,
    after: Option<String>,
}
fn default_limit() -> i64 {
    50
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ChangesInput {
    #[serde(default)]
    after: i64,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TaskRevision {
    id: String,
    revision: i64,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct UndoInput {
    seq: i64,
    revision: i64,
}

impl TodoerMcp {
    fn actor(&self, ctx: &RequestContext<RoleServer>) -> Result<String, ApiError> {
        let parts = ctx
            .extensions
            .get::<axum::http::request::Parts>()
            .ok_or_else(|| {
                ApiError(
                    axum::http::StatusCode::UNAUTHORIZED,
                    "unauthorized",
                    "MCP request authentication context required".into(),
                )
            })?;
        // Recheck the credential on every invocation, including calls made over
        // an already initialized session after revocation or expiry.
        self.state.mcp_actor(&parts.headers)
    }
}

#[tool_router]
impl TodoerMcp {
    #[tool(
        description = "Read/replace task scheduling (planned date, zoned due time, priority, reminders), manage root-leaf recurrence and pause/resume/stop/skip, or read explicit-date planning and weekly statistics. Use action.op; schedule set/start/skip use task revision, series_state uses series revision. Replacing a schedule clears omitted fields. No browser settings or credentials are changed."
    )]
    async fn schedule_tasks(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ScheduleToolInput>,
    ) -> CallToolResult {
        let actor = match self.actor(&ctx) {
            Ok(actor) => actor,
            Err(error) => return result::<()>(Err(error)),
        };
        result(
            self.state
                .run(move |db| {
                    let action = serde_json::from_value(p.action).map_err(|_| {
                        Error::Invalid("invalid schedule action; consult schema".into())
                    })?;
                    crate::web::agent_features::scheduling(db, &actor, action)
                })
                .await,
        )
    }
    #[tool(
        description = "Read/write authorized task comments and attachment metadata, create/resign bounded upload intents, verify/finalize uploaded objects, or return a checked download link. Comments default to300-character excerpts; explicit get reads one full comment. Never accepts/returns binary file bodies or object keys. Upload uses declared size/SHA256 and common server verification. Download links require a browser session or separate REST credential; MCP OAuth credentials are not accepted by REST."
    )]
    async fn task_activity(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ActivityToolInput>,
    ) -> CallToolResult {
        let actor = match self.actor(&ctx) {
            Ok(actor) => actor,
            Err(error) => return result::<()>(Err(error)),
        };
        let action = match serde_json::from_value(p.action) {
            Ok(action) => action,
            Err(_) => {
                return result::<()>(Err(Error::Invalid(
                    "invalid activity action; consult schema".into(),
                )
                .into()));
            }
        };
        result(crate::web::agent_features::activity(self.state.clone(), actor, action).await)
    }
    #[tool(
        description = "Create authorized workspaces/lists/sections, inspect/administer sharing, or create/list/revoke email-bound invitations. Grants and visibility expansion require explicit acknowledge_access_change and appropriate admin scope; revisions are workspace/list revisions. Invite requires retry-safe idempotency_key and may queue invitation email delivery. Never claims an invitation or bypasses Google/browser proof."
    )]
    async fn manage_access(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<AccessToolInput>,
    ) -> CallToolResult {
        let actor = match self.actor(&ctx) {
            Ok(actor) => actor,
            Err(error) => return result::<()>(Err(error)),
        };
        let action = match serde_json::from_value(p.action) {
            Ok(action) => action,
            Err(_) => {
                return result::<()>(Err(Error::Invalid(
                    "invalid access action; consult schema".into(),
                )
                .into()));
            }
        };
        result(crate::web::agent_features::access(self.state.clone(), actor, action).await)
    }
    #[tool(
        description = "Search authorized tasks, comments, attachment filenames, tags, sections, lists and workspaces with Unicode literal token prefixes. Structured filters include scope, status (default open), tags, dates in stored local timezone, priority and recurrence. Bounded projection and stable cursors; resync_required means restart. No fuzzy or semantic search."
    )]
    async fn query(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<QueryInput>,
    ) -> CallToolResult {
        let actor = match self.actor(&ctx) {
            Ok(actor) => actor,
            Err(error) => return result::<()>(Err(error)),
        };
        result(
            self.state
                .run(move |db| {
                    let query = serde_json::from_value(p.query).map_err(|_| {
                        Error::Invalid("invalid structured query; consult schema".into())
                    })?;
                    db.query(&actor, &query)
                })
                .await,
        )
    }
    #[tool(
        description = "Read/create/rename tags, replace task tags, read/assign root sections, reparent within a list, or reorder every active sibling exactly once. Use action.op. Writes use task revisions, tag_rename uses tag revision, reorder uses list revision. Standalone workspace-tag create requires workspace editor; direct list guests may introduce names during authorized assignment. Multi-step edits can use reviewed bulk changes."
    )]
    async fn organize_tasks(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<OrganizeInput>,
    ) -> CallToolResult {
        let actor = match self.actor(&ctx) {
            Ok(actor) => actor,
            Err(error) => return result::<()>(Err(error)),
        };
        result(
            self.state
                .run(move |db| {
                    let action = serde_json::from_value(p.action).map_err(|_| {
                        Error::Invalid("invalid organization action; consult schema".into())
                    })?;
                    crate::web::taxonomy_http::organize(db, &actor, action)
                })
                .await,
        )
    }
    #[tool(
        description = "Resolve exact @workspace/#list/$section/%task/+tag references, autocomplete accessible choices, review unresolved saved-source occurrences, explicitly bind a choice with current source revision, or read authorized backlinks. Resolve never chooses ambiguous candidates. Bind increments task/comment source revision and preserves other stable bindings. Code spans/blocks and existing links remain literal."
    )]
    async fn references(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ReferenceInput>,
    ) -> CallToolResult {
        let actor = match self.actor(&ctx) {
            Ok(actor) => actor,
            Err(error) => return result::<()>(Err(error)),
        };
        result(
            self.state
                .run(move |db| {
                    let action = serde_json::from_value(p.action).map_err(|_| {
                        Error::Invalid("invalid reference action; consult schema".into())
                    })?;
                    crate::web::taxonomy_http::references(db, &actor, action)
                })
                .await,
        )
    }
    #[tool(
        description = "Resolve an accessible exact list ID/title, or an explicit saved preference_key with optional inbox fallback. Returns top-level id/title on success; missing/ambiguous results are tool errors with accessible choices."
    )]
    async fn resolve_destination(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<Destination>,
    ) -> CallToolResult {
        let actor = match self.actor(&ctx) {
            Ok(actor) => actor,
            Err(error) => return result::<()>(Err(error)),
        };
        match self.state.run(move|db|db.resolve_destination(&actor,p.destination.as_deref(),p.preference_key.as_deref(),p.fallback_inbox)).await {
            Ok(resolved) if resolved.status=="resolved" => {
                let destination=resolved.destination.expect("resolved destination");
                result::<serde_json::Value>(Ok(json!({"id":destination.id,"title":destination.title,"status":"resolved"})))
            }
            Ok(resolved) => CallToolResult::error(vec![ContentBlock::text(json!({"error":{"code":resolved.status,"message":"Choose an explicit accessible destination ID"},"choices":resolved.choices,"has_more":resolved.has_more}).to_string())]),
            Err(error)=>result::<()>(Err(error)),
        }
    }
    #[tool(
        description = "Review 1–50 typed operations without saving business changes. Returns an actor-bound ten-minute preview/hash and compact before/after states. Use apply_changes only after reviewing; no create or completion operations."
    )]
    async fn preview_changes(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<PreviewChangesInput>,
    ) -> CallToolResult {
        let actor = match self.actor(&ctx) {
            Ok(actor) => actor,
            Err(error) => return result::<()>(Err(error)),
        };
        result(
            self.state
                .run(move |db| {
                    let operations = serde_json::from_value(serde_json::Value::Array(p.operations))
                        .map_err(|_| {
                            Error::Invalid(
                                "invalid typed bulk operations; consult the tool schema".into(),
                            )
                        })?;
                    db.bulk_preview(&actor, operations)
                })
                .await,
        )
    }
    #[tool(
        description = "Atomically apply a reviewed server-stored preview using its exact preview_id/hash and a retry-safe idempotency_key. Live authorization, task/list state and audience changes are rechecked. Stale or expired previews require a new review."
    )]
    async fn apply_changes(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ApplyChangesInput>,
    ) -> CallToolResult {
        let actor = match self.actor(&ctx) {
            Ok(actor) => actor,
            Err(error) => return result::<()>(Err(error)),
        };
        result(
            self.state
                .run(move |db| {
                    db.bulk_apply(&actor, &p.preview_id, &p.expected_hash, &p.idempotency_key)
                })
                .await,
        )
    }
    #[tool(
        description = "Read accessible saved destination preferences with no arguments, or save one by supplying both key and list_id. Requires current write access to the saved list; never stores a guessed destination or conversation state."
    )]
    async fn configure_destination(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<DestinationPreferenceInput>,
    ) -> CallToolResult {
        let actor = match self.actor(&ctx) {
            Ok(actor) => actor,
            Err(error) => return result::<()>(Err(error)),
        };
        result(
            self.state
                .run(move |db| match (p.key, p.list_id) {
                    (None, None) => serde_json::to_value(db.destination_preferences(&actor)?)
                        .map_err(|_| Error::Invalid("cannot encode preferences".into())),
                    (Some(key), Some(id)) => {
                        serde_json::to_value(db.set_destination_preference(&actor, &key, &id)?)
                            .map_err(|_| Error::Invalid("cannot encode preference".into()))
                    }
                    _ => Err(Error::Invalid(
                        "provide both key and list_id, or neither to read preferences".into(),
                    )),
                })
                .await,
        )
    }
    #[tool(
        description = "Capture a batch into an accessible list. Reuse an idempotency_key only for an identical retry; parent_id must identify a task in that list."
    )]
    async fn capture(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<CaptureInput>,
    ) -> CallToolResult {
        let actor = match self.actor(&ctx) {
            Ok(actor) => actor,
            Err(error) => return result::<()>(Err(error)),
        };
        result(
            self.state
                .run(move |db| {
                    if p.idempotency_key.is_empty() || p.idempotency_key.len() > 200 {
                        return Err(Error::Invalid(
                            "Idempotency key required (1–200 characters)".into(),
                        ));
                    }
                    db.capture(
                        &actor,
                        &p.idempotency_key,
                        Capture {
                            list_id: p.list_id,
                            tasks: p
                                .tasks
                                .into_iter()
                                .map(|task| NewTask {
                                    title: task.title,
                                    description: task.description,
                                    parent_id: task.parent_id,
                                })
                                .collect(),
                        },
                    )
                })
                .await,
        )
    }
    #[tool(
        description = "Read a task and its descendants, including IDs, parent links and current revisions."
    )]
    async fn read_subtree(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<TaskId>,
    ) -> CallToolResult {
        let actor = match self.actor(&ctx) {
            Ok(actor) => actor,
            Err(error) => return result::<()>(Err(error)),
        };
        result(self.state.run(move |db| db.subtree(&actor, &p.id)).await)
    }
    #[tool(
        description = "Update a task using its current revision. Set complete_subtree explicitly to complete descendants. On conflict re-read before retrying."
    )]
    async fn update_task(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<UpdateInput>,
    ) -> CallToolResult {
        let actor = match self.actor(&ctx) {
            Ok(actor) => actor,
            Err(error) => return result::<()>(Err(error)),
        };
        result(
            self.state
                .run(move |db| {
                    db.update(
                        &actor,
                        &p.id,
                        Change {
                            revision: p.revision,
                            title: p.title,
                            description: p.description,
                            complete: p.complete,
                            complete_subtree: p.complete_subtree,
                        },
                    )
                })
                .await,
        )
    }
    #[tool(
        description = "Preview who gains access when moving a subtree to a destination list ID. Review before setting acknowledge_access_change on move_subtree."
    )]
    async fn move_preview(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<MovePreviewInput>,
    ) -> CallToolResult {
        let actor = match self.actor(&ctx) {
            Ok(actor) => actor,
            Err(error) => return result::<()>(Err(error)),
        };
        result(
            self.state
                .run(move |db| db.move_preview(&actor, &p.id, &p.destination))
                .await,
        )
    }
    #[tool(
        description = "Move an entire subtree to a list ID using its current revision. A move that adds access requires explicit acknowledge_access_change=true after reviewing the audience change."
    )]
    async fn move_subtree(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<MoveInput>,
    ) -> CallToolResult {
        let actor = match self.actor(&ctx) {
            Ok(actor) => actor,
            Err(error) => return result::<()>(Err(error)),
        };
        result(
            self.state
                .run(move |db| {
                    db.move_subtree(
                        &actor,
                        &p.id,
                        &p.destination,
                        p.revision,
                        p.acknowledge_access_change,
                    )
                })
                .await,
        )
    }
    #[tool(
        description = "Search accessible tasks by text, optional list ID and completion status. Limit is 1–100 (default 50); pass the last task ID as after for the next page."
    )]
    async fn search(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SearchInput>,
    ) -> CallToolResult {
        let actor = match self.actor(&ctx) {
            Ok(actor) => actor,
            Err(error) => return result::<()>(Err(error)),
        };
        result(
            self.state
                .run(move |db| {
                    db.search(
                        &actor,
                        &p.query,
                        p.list.as_deref(),
                        p.complete,
                        p.limit,
                        p.after.as_deref(),
                    )
                })
                .await,
        )
    }
    #[tool(
        description = "List accessible trashed tasks. Use current revisions and the original trash operation root when restoring."
    )]
    async fn trash_tasks(&self, ctx: RequestContext<RoleServer>) -> CallToolResult {
        let actor = match self.actor(&ctx) {
            Ok(actor) => actor,
            Err(error) => return result::<()>(Err(error)),
        };
        result(self.state.run(move |db| db.trash_tasks(&actor)).await)
    }
    #[tool(
        description = "Trash a task and its active descendants atomically using the root current revision. This hides the subtree from active task reads and search."
    )]
    async fn trash_task(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<TaskRevision>,
    ) -> CallToolResult {
        let actor = match self.actor(&ctx) {
            Ok(actor) => actor,
            Err(error) => return result::<()>(Err(error)),
        };
        result(
            self.state
                .run(move |db| db.trash(&actor, &p.id, p.revision))
                .await,
        )
    }
    #[tool(
        description = "Restore a trashed subtree using the original trash operation root ID and its exact current revision. Independently trashed descendants remain separate; re-read on conflict."
    )]
    async fn restore_task(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<TaskRevision>,
    ) -> CallToolResult {
        let actor = match self.actor(&ctx) {
            Ok(actor) => actor,
            Err(error) => return result::<()>(Err(error)),
        };
        result(
            self.state
                .run(move |db| db.restore(&actor, &p.id, p.revision))
                .await,
        )
    }
    #[tool(
        description = "Compensate one eligible individual update event by sequence. Requires the original actor, current access, and the exact current task revision unchanged since that event. Never undoes a whole cascade or overwrites newer work."
    )]
    async fn undo_update(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<UndoInput>,
    ) -> CallToolResult {
        let actor = match self.actor(&ctx) {
            Ok(actor) => actor,
            Err(error) => return result::<()>(Err(error)),
        };
        result(
            self.state
                .run(move |db| db.undo_update(&actor, p.seq, p.revision))
                .await,
        )
    }
    #[tool(
        description = "Read the accessible change feed after an event sequence number; default 0."
    )]
    async fn changes(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ChangesInput>,
    ) -> CallToolResult {
        let actor = match self.actor(&ctx) {
            Ok(actor) => actor,
            Err(error) => return result::<()>(Err(error)),
        };
        result(self.state.run(move |db| db.changes(&actor, p.after)).await)
    }
}
#[tool_handler(router = self.tool_router)]
impl ServerHandler for TodoerMcp {
    fn get_info(&self) -> ServerConfig {
        let mut config = ServerConfig::new(ServerCapabilities::builder().enable_tools().build());
        config.instructions = Some("Every tool checks the current authenticated actor and live credential. Resolve list IDs before writes. Re-read on revision conflicts. Review access changes before acknowledging subtree moves.".into());
        config
    }
}
