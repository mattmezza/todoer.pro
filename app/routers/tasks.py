import hashlib
from typing import Annotated, Mapping

from databases import Database
from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse, PlainTextResponse
from fastapi.templating import Jinja2Templates

from app import blob, http, templating
from app.dependencies import (
    RepoDB,
    get_current_active_user,
    get_db,
    get_tpl,
    repo_db,
)
from app.ids import new_ids

router = APIRouter()


@router.post("/tasks")
@templating.htmx("Tasks.TaskItem")
async def add_task(
    req: Request,
    title: Annotated[str, Form()],
    section: Annotated[str, Form()],
    descr: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    uuid_new, sid_new = new_ids()
    task = await db.for_user(user).tasks().create(title, descr, section)
    return {"task": task}, 201, {"HX-Trigger": "task-create"}


@router.get("/task/{sid}", response_class=HTMLResponse)
async def view(
    req: Request,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    # TODO - refactor all these damn DB queries
    # I'm pretty sure they can be improved ;)
    task = await db.fetch_one(
        """
        SELECT * FROM tasks WHERE sid = :sid AND section IN (
            SELECT uuid from sections WHERE list IN (
                SELECT uuid FROM lists WHERE creator = :creator
            )
        )
        """,
        {"sid": sid, "creator": user['uuid']},
    )
    se = await db.fetch_one(
        """
        SELECT * FROM sections WHERE uuid = :uuid AND list IN (
            SELECT uuid FROM lists WHERE creator = :creator
        )
        """,
        {"uuid": task.section, "creator": user['uuid']},
    )
    li = await db.fetch_one(
        "SELECT * FROM lists WHERE uuid = :uuid AND creator = :creator",
        {"uuid": se.list, "creator": user['uuid']},
    )
    cc = await db.fetch_all(
        """
        SELECT cc.*, uu.email as author_email FROM comments cc
            JOIN users uu on cc.author = uu.uuid
        WHERE cc.task = :task AND cc.task IN (
            SELECT uuid from tasks WHERE section IN (
                SELECT uuid from sections WHERE list IN (
                    SELECT uuid FROM lists WHERE creator = :creator
                )
            )
        )
        LIMIT 100
        """,
        {"task": task.uuid, "creator": user['uuid']},
    )
    cc = [dict(c._mapping) for c in cc]
    for c in cc:
        c["author_email_hash"] = hashlib.md5(
            str(c["author_email"]).encode("utf-8")
        ).hexdigest()
    comment_tpl = tpl.get_template("comments/view.htm")
    subtasks = await db.fetch_all(
        """
        SELECT * FROM subtasks WHERE task = :task AND task IN (
            SELECT uuid from tasks WHERE section IN (
                SELECT uuid from sections WHERE list IN (
                    SELECT uuid FROM lists WHERE creator = :creator
                )
            )
        )
        """,
        {"task": task.uuid, "creator": user['uuid']},
    )
    subtasks = [dict(s._mapping) for s in subtasks]
    subtask_tpl = tpl.get_template("subtasks/view.htm")
    attachments = await db.fetch_all(
        """
        SELECT sid, uuid, mime, fname, size, created FROM attachments
        WHERE task = :task and task IN (
            SELECT uuid FROM tasks WHERE section IN (
                SELECT uuid FROM sections WHERE list IN (
                    SELECT uuid FROM lists WHERE creator = :creator
                )
            )
        )
        LIMIT 100
        """,
        {"task": task.uuid, "creator": user['uuid']},
    )
    attachments = [dict(a._mapping) for a in attachments]
    for a in attachments:
        a['url'] = blob.url_signed_for(
            a['uuid'],
            blob.URL_SIGNED_VALIDITY_DURATION_DEFAULT,
        )
    attachments_tpl = tpl.get_template("attachments/view.htm")
    title_tpl = tpl.get_template("tasks/title.htm")
    return tpl.TemplateResponse(
        "tasks/detail.htm",
        templating.constants
        | {
            "request": req,
            "task": task._mapping,
            "section": se._mapping,
            "list": li._mapping,
            "comments": cc,
            "subtasks": subtasks,
            "attachments": attachments,
            "user": user,
            "tpls": {
                "title": {"view": title_tpl.render},
                "comments": {"view": comment_tpl.render},
                "subtasks": {"view": subtask_tpl.render},
                "attachments": {"view": attachments_tpl.render},
            },
        },
    )


@router.put("/task/{sid}")
@templating.htmx("Tasks.TaskItem")
async def update(
    sid: str,
    title: Annotated[str, Form()],
    descr: Annotated[str | None, Form()] = None,
    complete: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    task = http.Opt(
        await db.for_user(user)
        .tasks()
        .update_task_by_sid(complete, title, descr, sid)
    ).or_404(f"Task #{sid} not found.")
    hx_trigger = f"task-update, task-{sid}-update"
    if task.complete == 1:
        hx_trigger += f", task-complete task-{sid}-complete"
    else:
        hx_trigger += f", task-incomplete, task-{sid}-incomplete"
    return {"task": task}, 200, {"HX-Trigger": hx_trigger}


@router.put("/task/{sid}/descr", response_class=PlainTextResponse)
async def update_descr(
    res: PlainTextResponse,
    sid: str,
    descr: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    descr = await db.for_user(user).tasks().update_descr_by_sid(descr, sid)
    res.headers["HX-Trigger"] = f"task-update, task-{sid}-update"
    return descr


@router.put("/task/{sid}/complete")
@templating.htmx("Tasks.Title")
async def update_complete(
    sid: str,
    complete: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    task = http.Opt(
        urepo.tasks().update_completed_by_sid(complete, sid)
    ).or_404(f"Task #{sid} not found.")
    section = await urepo.sections().by_uuid(task["section"])
    list_ = await urepo.lists().by_uuid(section["list"])
    space = await urepo.spaces().by_uuid(list_["space"])
    return (
        {
            "task": task,
            "section": section,
            "list": list_,
            "space": space,
        },
        200,
        {"HX-Trigger": f"task-update, task-{sid}-update"},
    )


@router.put("/task/{sid}/title")
@templating.htmx("Tasks.Title")
async def update_title(
    sid: str,
    title: Annotated[str, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    urepo = db.for_user(user)
    task = http.Opt(urepo.tasks().update_title_by_sid(title, sid)).or_404(
        f"Task #{sid} not found."
    )
    section = await urepo.sections().by_uuid(task["section"])
    list_ = await urepo.lists().by_uuid(section["list"])
    space = await urepo.spaces().by_uuid(list_["space"])
    return (
        {
            "task": task,
            "section": section,
            "list": list_,
            "space": space,
        },
        200,
        {"HX-Trigger": f"task-update, task-{sid}-update"},
    )


@router.delete("/task/{sid}", response_class=HTMLResponse)
async def delete(
    res: HTMLResponse,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    await db.for_user(user).tasks().del_by_sid(sid)
    res.headers["HX-Trigger"] = f"task-delete, task-{sid}-delete"
    return res
