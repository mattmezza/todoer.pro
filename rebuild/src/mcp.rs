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
