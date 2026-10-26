//! Compact client operations share domain writes and attachment verification.
use super::*;
use crate::{
    discussion::{EditComment, NewComment, UploadInput},
    invitations::InviteInput,
    scheduling::{RecurrenceSpec, Schedule, SeriesState},
    workspace::{
        CreateList, ListRole, ListWorkspaceMoveInput, SectionInput, WorkspaceInput, WorkspaceRole,
    },
};
use serde::Serialize;
fn default_limit() -> i64 {
    20
}
fn value<T: Serialize>(v: T) -> Result<Value, Error> {
    serde_json::to_value(v).map_err(|_| Error::Invalid("cannot encode operation response".into()))
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ScheduleAction {
    Get {
        id: String,
    },
    Set {
        id: String,
        revision: i64,
        schedule: Schedule,
    },
    SeriesStart {
        id: String,
        revision: i64,
        spec: RecurrenceSpec,
    },
    SeriesGet {
        id: String,
    },
    SeriesList,
    SeriesState {
        id: String,
        revision: i64,
        state: SeriesState,
    },
    Occurrences {
        id: String,
    },
    Skip {
        id: String,
        revision: i64,
    },
    Planning {
        from: String,
        to: String,
    },
    Statistics {
        from: String,
    },
    Review {
        from: String,
        to: String,
        list_id: Option<String>,
        workspace_id: Option<String>,
    },
}
impl ScheduleAction {
    fn writes(&self) -> bool {
        matches!(
            self,
            Self::Set { .. }
                | Self::SeriesStart { .. }
                | Self::SeriesState { .. }
                | Self::Skip { .. }
        )
    }
}
pub(crate) fn scheduling(
    db: &mut Store,
    actor: &str,
    action: ScheduleAction,
) -> Result<Value, Error> {
    match action {
        ScheduleAction::Get { id } => value(db.get_schedule(actor, &id)?),
        ScheduleAction::Set {
            id,
            revision,
            schedule,
        } => value(db.set_schedule(actor, &id, revision, schedule)?),
        ScheduleAction::SeriesStart { id, revision, spec } => {
            value(db.start_series(actor, &id, revision, spec)?)
        }
        ScheduleAction::SeriesGet { id } => value(db.series(actor, &id)?),
        ScheduleAction::SeriesList => {
            let mut series = db.recurring_series(actor)?;
            let truncated = series.len() > 100;
            series.truncate(100);
            Ok(json!({"series":series,"truncated":truncated}))
        }
        ScheduleAction::SeriesState {
            id,
            revision,
            state,
        } => value(db.set_series_state(
            actor,
            &id,
            revision,
            state,
            chrono::Utc::now().timestamp(),
        )?),
        ScheduleAction::Occurrences { id } => value(db.occurrences(actor, &id)?),
        ScheduleAction::Skip { id, revision } => {
            value(db.skip_occurrence(actor, &id, revision, chrono::Utc::now().timestamp())?)
        }
        ScheduleAction::Planning { from, to } => {
            let rows = db.planning_tasks(actor, &from, &to)?;
            let truncated = rows.len() > 100;
            let tasks=rows.into_iter().take(100).map(|row|json!({"task":{"id":row.task.id,"reference":row.task.reference,"list_id":row.task.list_id,"parent_id":row.task.parent_id,"title":row.task.title,"complete":row.task.complete,"revision":row.task.revision},"schedule":row.schedule,"dates":row.dates})).collect::<Vec<_>>();
            Ok(json!({"tasks":tasks,"truncated":truncated}))
        }
        ScheduleAction::Statistics { from } => value(db.weekly_statistics(actor, &from)?),
        ScheduleAction::Review {
            from,
            to,
            list_id,
            workspace_id,
        } => {
            let mut review = db.weekly_review(
                actor,
                crate::statistics::ReviewRange {
                    from,
                    to,
                    list_id,
                    workspace_id,
                },
                chrono::Utc::now().timestamp(),
            )?;
            for commitment in &mut review.upcoming {
                commitment.task.description.clear();
            }
            review.notes.push(
                "Agent review omits task descriptions; read an individual task for full context."
                    .into(),
            );
            value(review)
        }
    }
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ActivityAction {
    Comments {
        task_id: String,
        #[serde(default)]
        after_seq: i64,
        #[serde(default = "default_limit")]
        limit: i64,
        #[serde(default)]
        include_bodies: bool,
    },
    CommentGet {
        id: String,
    },
    CommentCreate {
        task_id: String,
        idempotency_key: String,
        body: String,
    },
    CommentEdit {
        id: String,
        revision: i64,
        body: String,
    },
    CommentDelete {
        id: String,
        revision: i64,
    },
    Files {
        task_id: String,
    },
    FileDelete {
        id: String,
        revision: i64,
    },
    FileLink {
        id: String,
    },
    UploadCreate {
        task_id: String,
        input: UploadInput,
    },
    UploadGet {
        id: String,
    },
    UploadFinalize {
        id: String,
        revision: i64,
    },
}
impl ActivityAction {
    fn writes(&self) -> bool {
        matches!(
            self,
            Self::CommentCreate { .. }
                | Self::CommentEdit { .. }
                | Self::CommentDelete { .. }
                | Self::FileDelete { .. }
                | Self::UploadCreate { .. }
                | Self::UploadGet { .. }
                | Self::UploadFinalize { .. }
        )
    }
}
pub(crate) async fn activity(
    s: AppState,
    actor: String,
    action: ActivityAction,
) -> Result<Value, ApiError> {
    match action {
        ActivityAction::UploadCreate { task_id, input } => {
            super::discussion_http::create_upload_actor(s, actor, task_id, input).await
        }
        ActivityAction::UploadGet { id } => {
            super::discussion_http::upload_actor(s, actor, id).await
        }
        ActivityAction::UploadFinalize { id, revision } => Ok(value(
            super::discussion_http::finalize_actor(s, actor, id, revision).await?,
        )?),
        ActivityAction::FileLink { id } => {
            let attachment = s.run(move |db| db.attachment(&actor, &id)).await?;
            let mut url = reqwest::Url::parse(&s.origin)
                .map_err(|_| Error::Invalid("invalid application origin".into()))?;
            url.path_segments_mut()
                .map_err(|_| Error::Invalid("invalid application origin".into()))?
                .extend(["api", "v1", "attachments", &attachment.id, "download"]);
            Ok(
                json!({"attachment":attachment,"download_url":url.as_str(),"authentication":"browser_session_or_separate_rest_credential","mcp_oauth_credential_accepted":false,"storage_configured":s.storage.is_some()}),
            )
        }
        action => {
            s.run(move |db| match action {
                ActivityAction::Comments {
                    task_id,
                    after_seq,
                    limit,
                    include_bodies,
                } => {
                    if !(1..=50).contains(&limit) {
                        return Err(Error::Invalid("comment page limit must be 1..50".into()));
                    }
                    let limit = if include_bodies { limit.min(10) } else { limit };
                    let comments = db.comments(&actor, &task_id, after_seq, limit)?;
                    let next = comments.last().map(|c| c.seq);
                    let comments = comments
                        .into_iter()
                        .map(|comment| {
                            let body = comment.body.clone();
                            let mut result = serde_json::to_value(comment).expect("comment JSON");
                            if !include_bodies {
                                result
                                    .as_object_mut()
                                    .expect("comment object")
                                    .remove("body");
                                result["body_excerpt"] =
                                    Value::String(body.chars().take(300).collect());
                                result["body_truncated"] = Value::Bool(body.chars().count() > 300);
                            }
                            result
                        })
                        .collect::<Vec<_>>();
                    Ok(json!({"comments":comments,"next_after_seq":next,"page_limit":limit}))
                }
                ActivityAction::CommentGet { id } => value(db.comment(&actor, &id)?),
                ActivityAction::CommentCreate {
                    task_id,
                    idempotency_key,
                    body,
                } => value(db.create_comment(
                    &actor,
                    &task_id,
                    &idempotency_key,
                    NewComment { body },
                )?),
                ActivityAction::CommentEdit { id, revision, body } => {
                    value(db.update_comment(&actor, &id, EditComment { revision, body })?)
                }
                ActivityAction::CommentDelete { id, revision } => {
                    value(db.delete_comment(&actor, &id, revision)?)
                }
                ActivityAction::Files { task_id } => {
                    let mut files = db.attachments(&actor, &task_id)?;
                    let truncated = files.len() > 100;
                    files.truncate(100);
                    Ok(json!({"attachments":files,"truncated":truncated}))
                }
                ActivityAction::FileDelete { id, revision } => {
                    value(db.delete_attachment(&actor, &id, revision)?)
                }
                _ => unreachable!("storage operations dispatched before database closure"),
            })
            .await
        }
    }
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum AccessAction {
    WorkspaceCreate {
        input: WorkspaceInput,
    },
    ListCreate {
        input: CreateList,
    },
    SectionCreate {
        list_id: String,
        input: SectionInput,
    },
    WorkspaceMembers {
        workspace_id: String,
    },
    WorkspaceGrant {
        workspace_id: String,
        user_id: String,
        role: WorkspaceRole,
        revision: i64,
        #[serde(default)]
        acknowledge_access_change: bool,
    },
    WorkspaceRevoke {
        workspace_id: String,
        user_id: String,
        revision: i64,
    },
    ListInfo {
        list_id: String,
    },
    ListMovePreview {
        list_id: String,
        destination_workspace_id: String,
    },
    ListMove {
        list_id: String,
        input: ListWorkspaceMoveInput,
    },
    ListGrant {
        list_id: String,
        user_id: String,
        role: ListRole,
        revision: i64,
        #[serde(default)]
        acknowledge_access_change: bool,
    },
    ListRevoke {
        list_id: String,
        user_id: String,
        revision: i64,
    },
    ListRestricted {
        list_id: String,
        restricted: bool,
        revision: i64,
        #[serde(default)]
        acknowledge_access_change: bool,
    },
    Invitations {
        target_kind: String,
        target_id: String,
    },
    Invite {
        idempotency_key: String,
        input: InviteInput,
    },
    RevokeInvitation {
        id: String,
    },
}
impl AccessAction {
    fn writes(&self) -> bool {
        !matches!(
            self,
            Self::WorkspaceMembers { .. } | Self::ListInfo { .. } | Self::Invitations { .. }
        )
    }
}
pub(crate) async fn access(
    s: AppState,
    actor: String,
    action: AccessAction,
) -> Result<Value, ApiError> {
    let signer = if matches!(&action, AccessAction::Invite { .. }) {
        Some(s.invitations.clone().ok_or(ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "invitations_disabled",
            "Invitation signing is not configured".into(),
        ))?)
    } else {
        None
    };
    s.run(move |db| match action {
        AccessAction::WorkspaceCreate { input } => value(db.create_workspace(&actor, input)?),
        AccessAction::ListCreate { input } => value(db.create_list(&actor, input)?),
        AccessAction::SectionCreate { list_id, input } => {
            value(db.create_section(&actor, &list_id, input)?)
        }
        AccessAction::WorkspaceMembers { workspace_id } => {
            let mut members = db.workspace_members(&actor, &workspace_id)?;
            let truncated = members.len() > 100;
            members.truncate(100);
            Ok(json!({"members":members,"truncated":truncated}))
        }
        AccessAction::WorkspaceGrant {
            workspace_id,
            user_id,
            role,
            revision,
            acknowledge_access_change,
        } => value(db.set_workspace_member(
            &actor,
            &workspace_id,
            &user_id,
            role,
            revision,
            acknowledge_access_change,
        )?),
        AccessAction::WorkspaceRevoke {
            workspace_id,
            user_id,
            revision,
        } => value(db.remove_workspace_member(&actor, &workspace_id, &user_id, revision)?),
        AccessAction::ListInfo { list_id } => value(db.list_info(&actor, &list_id)?),
        AccessAction::ListMovePreview {
            list_id,
            destination_workspace_id,
        } => value(db.list_workspace_move_preview(&actor, &list_id, &destination_workspace_id)?),
        AccessAction::ListMove { list_id, input } => {
            value(db.move_list_workspace(&actor, &list_id, input)?)
        }
        AccessAction::ListGrant {
            list_id,
            user_id,
            role,
            revision,
            acknowledge_access_change,
        } => value(db.grant_list(
            &actor,
            &list_id,
            &user_id,
            role,
            revision,
            acknowledge_access_change,
        )?),
        AccessAction::ListRevoke {
            list_id,
            user_id,
            revision,
        } => value(db.revoke_list(&actor, &list_id, &user_id, revision)?),
        AccessAction::ListRestricted {
            list_id,
            restricted,
            revision,
            acknowledge_access_change,
        } => value(db.set_list_restricted(
            &actor,
            &list_id,
            restricted,
            revision,
            acknowledge_access_change,
        )?),
        AccessAction::Invitations {
            target_kind,
            target_id,
        } => value(db.invitations(&actor, &target_kind, &target_id)?),
        AccessAction::Invite {
            idempotency_key,
            input,
        } => {
            let signer = signer
                .ok_or_else(|| Error::Invalid("invitation signing is not configured".into()))?;
            value(db.create_invitation(&actor, &idempotency_key, input, &signer)?)
        }
        AccessAction::RevokeInvitation { id } => {
            db.revoke_invitation(&actor, &id)?;
            Ok(json!({"revoked":true}))
        }
    })
    .await
}
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/agent/schedule", post(schedule_request))
        .route("/api/v1/agent/activity", post(activity_request))
        .route("/api/v1/agent/access", post(access_request))
}
async fn schedule_request(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(action): Input<ScheduleAction>,
) -> Result<Json<Value>, ApiError> {
    let actor = s.auth(&h, action.writes())?;
    Ok(Json(s.run(move |db| scheduling(db, &actor, action)).await?))
}
async fn activity_request(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(action): Input<ActivityAction>,
) -> Result<Json<Value>, ApiError> {
    let actor = s.auth(&h, action.writes())?;
    Ok(Json(activity(s, actor, action).await?))
}
async fn access_request(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(action): Input<AccessAction>,
) -> Result<Json<Value>, ApiError> {
    let actor = s.auth(&h, action.writes())?;
    Ok(Json(access(s, actor, action).await?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    const TOKEN: &str = "isolated-agent-feature-http-token";
    fn app() -> (Router, Arc<Mutex<Store>>, String) {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let id = db
            .capture(
                "dev",
                "feature-test",
                Capture {
                    list_id: "inbox".into(),
                    tasks: vec![crate::core::NewTask {
                        title: "Feature task".into(),
                        description: String::new(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0)
            .id;
        let db = Arc::new(Mutex::new(db));
        let state = AppState::new(db.clone(), TOKEN.into(), "http://127.0.0.1:8092".into())
            .with_invitations(
                crate::invitations::InvitationSigner::new(
                    &"a".repeat(64),
                    "http://127.0.0.1:8092".into(),
                )
                .unwrap(),
            );
        (router(state), db, id)
    }
    async fn send(app: &Router, path: &str, body: Value, auth: bool) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/json");
        if auth {
            req = req.header("authorization", format!("Bearer {TOKEN}"));
        }
        let res = app
            .clone()
            .oneshot(req.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }
    #[tokio::test]
    async fn schedule_activity_routes_preserve_revisions_retries_and_compact_projection() {
        let (app, db, id) = app();
        assert_eq!(
            send(
                &app,
                "/api/v1/agent/schedule",
                json!({"op":"get","id":id}),
                false
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
        let(status,state)=send(&app,"/api/v1/agent/schedule",json!({"op":"set","id":id,"revision":1,"schedule":{"planned":{"kind":"date","date":"2026-10-26"},"priority":2}}),true).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(state["task_revision"], 2);
        let (_, planned) = send(
            &app,
            "/api/v1/agent/schedule",
            json!({"op":"planning","from":"2026-10-26","to":"2026-10-27"}),
            true,
        )
        .await;
        assert_eq!(planned["tasks"][0]["task"]["id"], id);
        assert!(planned["tasks"][0]["task"].get("description").is_none());

        assert_eq!(
            send(
                &app,
                "/api/v1/agent/schedule",
                json!({"op":"set","id":id,"revision":1,"schedule":{}}),
                true
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        let body = json!({"op":"comment_create","task_id":id,"idempotency_key":"same-comment","body":"x".repeat(500)});
        let (status, comment) = send(&app, "/api/v1/agent/activity", body.clone(), true).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            send(&app, "/api/v1/agent/activity", body, true).await.1,
            comment
        );
        let (_, page) = send(
            &app,
            "/api/v1/agent/activity",
            json!({"op":"comments","task_id":id}),
            true,
        )
        .await;
        assert!(page["comments"][0].get("body").is_none());
        assert_eq!(
            page["comments"][0]["body_excerpt"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            300
        );
        assert_eq!(page["comments"][0]["body_truncated"], true);
        let (_, full) = send(
            &app,
            "/api/v1/agent/activity",
            json!({"op":"comments","task_id":id,"include_bodies":true}),
            true,
        )
        .await;
        assert_eq!(full["page_limit"], 10);
        assert_eq!(full["comments"][0]["body"].as_str().unwrap().len(), 500);
        let cid = comment["id"].as_str().unwrap();
        assert_eq!(
            send(
                &app,
                "/api/v1/agent/activity",
                json!({"op":"comment_edit","id":cid,"revision":1,"body":"Updated"}),
                true
            )
            .await
            .1["revision"],
            2
        );
        assert_eq!(
            send(
                &app,
                "/api/v1/agent/activity",
                json!({"op":"comment_edit","id":cid,"revision":1,"body":"Stale"}),
                true
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        db.lock()
            .unwrap()
            .conn
            .execute(
                "INSERT INTO membership_roles VALUES('inbox','dev','viewer')",
                [],
            )
            .unwrap();
        assert_eq!(
            send(
                &app,
                "/api/v1/agent/activity",
                json!({"op":"comment_delete","id":cid,"revision":2}),
                true
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
    #[tokio::test]
    async fn access_routes_require_acknowledgement_and_bind_invitation_retry_payload() {
        let (app, _, _) = app();
        let (status, w) = send(
            &app,
            "/api/v1/agent/access",
            json!({"op":"workspace_create","input":{"title":"Synthetic shared"}}),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let grant = json!({"op":"workspace_grant","workspace_id":w["id"],"user_id":"other","role":"viewer","revision":w["revision"]});
        assert_eq!(
            send(&app, "/api/v1/agent/access", grant.clone(), true)
                .await
                .0,
            StatusCode::CONFLICT
        );
        let mut acknowledged = grant;
        acknowledged["acknowledge_access_change"] = json!(true);
        assert_eq!(
            send(&app, "/api/v1/agent/access", acknowledged, true)
                .await
                .0,
            StatusCode::OK
        );
        let mut invite = json!({"op":"invite","idempotency_key":"synthetic-invite","input":{"target_kind":"workspace","target_id":w["id"],"email":"synthetic@example.invalid","role":"viewer","acknowledge_access_change":false}});
        assert_eq!(
            send(&app, "/api/v1/agent/access", invite.clone(), true)
                .await
                .0,
            StatusCode::CONFLICT
        );
        invite["input"]["acknowledge_access_change"] = json!(true);
        let (status, result) = send(&app, "/api/v1/agent/access", invite.clone(), true).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            send(&app, "/api/v1/agent/access", invite.clone(), true)
                .await
                .1,
            result
        );
        invite["input"]["email"] = json!("different@example.invalid");
        assert_eq!(
            send(&app, "/api/v1/agent/access", invite, true).await.0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            send(
                &app,
                "/api/v1/agent/access",
                json!({"op":"revoke_invitation","id":result["invitation"]["id"]}),
                true
            )
            .await
            .1,
            json!({"revoked":true})
        );
    }
    #[tokio::test]
    async fn file_links_are_authorized_metadata_not_mcp_download_grants() {
        let (app, db, id) = app();
        let intent = db
            .lock()
            .unwrap()
            .create_upload_intent(
                "dev",
                &id,
                UploadInput {
                    comment_id: None,
                    filename: "synthetic.txt".into(),
                    size: 0,
                    sha256: crate::object_storage::sha256_hex(b""),
                },
            )
            .unwrap();
        let file = db
            .lock()
            .unwrap()
            .finalize_attachment(
                "dev",
                crate::object_storage::VerifiedObject::synthetic(&intent),
            )
            .unwrap();
        let (status, link) = send(
            &app,
            "/api/v1/agent/activity",
            json!({"op":"file_link","id":file.id}),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(link["mcp_oauth_credential_accepted"], false);
        assert_eq!(link["storage_configured"], false);
        assert!(!link.to_string().contains("object_key"));
        assert!(
            link["download_url"]
                .as_str()
                .unwrap()
                .ends_with("/download")
        );
        db.lock()
            .unwrap()
            .conn
            .execute(
                "DELETE FROM memberships WHERE list_id='inbox' AND user_id='dev'",
                [],
            )
            .unwrap();
        assert_eq!(
            send(
                &app,
                "/api/v1/agent/activity",
                json!({"op":"file_link","id":file.id}),
                true
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
}
