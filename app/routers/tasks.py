import hashlib
from datetime import datetime
from typing import Annotated, Mapping

from databases import Database
from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates

from app.age import age
from app.dependencies import get_current_active_user, get_db, get_tpl
from app.ids import new_ids
from app import blob, templating

router = APIRouter()


@router.post("", response_class=HTMLResponse)
async def add_task(
    req: Request,
    title: Annotated[str, Form()],
    section: Annotated[str, Form()],
    descr: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    uuid_new, sid_new = new_ids()
    task = await db.fetch_one(
        """
        INSERT INTO tasks (uuid, sid, `order`, title, descr, section)
        VALUES (
            :uuid,
            :sid,
            COALESCE(
                (select MAX(`order`) + 1 FROM tasks WHERE section = :section),
                0
            ),
            :title,
            :descr,
            :section
        )
        RETURNING *
        """,
        {
            "uuid": uuid_new,
            "sid": sid_new,
            "title": title.strip(),
            "descr": descr.strip() if descr else None,
            "section": section,
        },
    )
    res = tpl.TemplateResponse(
        "tasks/view.htm",
        {"request": req, "task": task._mapping, "user": user},
    )
    res.headers["HX-Trigger"] = "task-create"
    return res


@router.get("/{sid}", response_class=HTMLResponse)
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
        templating.constants | {
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


@router.put("/{sid}", response_class=HTMLResponse)
async def update(
    req: Request,
    sid: str,
    title: Annotated[str, Form()],
    descr: Annotated[str | None, Form()] = None,
    complete: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    task = await db.fetch_one(
        """
        UPDATE tasks
        SET complete = :complete, title = :title, descr = :descr
        WHERE sid = :sid
        RETURNING *
        """,
        {
            "sid": sid,
            "complete": 1 if complete else 0,
            "title": title,
            "descr": descr,
        },
    )
    res = tpl.TemplateResponse(
        "tasks/view.htm",
        {"request": req, "task": task},
    )
    res.headers["HX-Trigger"] = "task-update"
    if task.complete == 1:
        res.headers["HX-Trigger"] = "task-complete"
    else:
        res.headers["HX-Trigger"] = "task-incomplete"
    return res


@router.put("/{sid}/descr", response_class=HTMLResponse)
async def update_descr(
    req: Request,
    sid: str,
    descr: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    row = await db.fetch_one(
        "UPDATE tasks SET descr = :descr WHERE sid = :sid RETURNING descr",
        {"sid": sid, "descr": descr},
    )
    return row.descr


@router.put("/{sid}/complete", response_class=HTMLResponse)
async def update_complete(
    req: Request,
    sid: str,
    complete: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    await db.fetch_one(
        """
            UPDATE tasks SET
                complete = :complete,
                completed = :completed,
                updated = CURRENT_TIMESTAMP
            WHERE sid = :sid
        """,
        {
            "sid": sid,
            "complete": 1 if complete else 0,
            "completed": datetime.utcnow() if complete else "NULL",
        },
    )
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
    return tpl.TemplateResponse(
        "tasks/title.htm",
        templating.constants | {
            "request": req,
            "task": task._mapping,
            "section": se._mapping,
            "list": li._mapping,
        },
    )


@router.put("/{sid}/title", response_class=HTMLResponse)
async def update_title(
    req: Request,
    sid: str,
    title: Annotated[str, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    row = await db.fetch_one(
        "UPDATE tasks SET title = :title WHERE sid = :sid RETURNING title",
        {"sid": sid, "title": title},
    )
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
    return tpl.TemplateResponse(
        "tasks/title.htm",
        templating.constants | {
            "request": req,
            "task": task._mapping,
            "section": se._mapping,
            "list": li._mapping,
        },
    )


@router.delete("/{sid}", response_class=HTMLResponse)
async def delete(
    req: Request,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    await db.execute("DELETE FROM tasks WHERE sid = :sid", {"sid": sid})
    res = HTMLResponse("")
    res.headers["HX-Trigger"] = "task-delete"
    return res
