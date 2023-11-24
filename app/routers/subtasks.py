from typing import Annotated, Mapping

from databases import Database
from fastapi import APIRouter, Depends, Request, Form
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates

from app.dependencies import get_current_active_user, get_db, get_tpl
from app.ids import new_ids

router = APIRouter()


@router.delete("/{sid}", response_class=HTMLResponse)
async def delete(
    req: Request,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
):
    await db.execute(
        """DELETE FROM subtasks
            WHERE sid = :sid AND task IN (
                SELECT uuid FROM tasks WHERE section IN (
                    SELECT uuid FROM sections WHERE creator = :creator
                )
            )
        """,
        {"sid": sid, "creator": user["uuid"]},
    )
    res = HTMLResponse("")
    res.headers["HX-Trigger"] = "subtask-delete"
    return res


@router.post("", response_class=HTMLResponse)
async def add(
    req: Request,
    task: Annotated[str, Form()],
    title: Annotated[str, Form()],
    descr: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    uuid_new, sid_new = new_ids()
    subt = await db.fetch_one(
        """INSERT INTO subtasks (
            uuid,
            sid,
            `order`,
            title,
            descr,
            task
        )
        VALUES (
            :uuid,
            :sid,
            (SELECT COALESCE(MAX(`order`), 0) FROM subtasks WHERE task = :task),
            :title,
            :descr,
            :task
        )
        RETURNING *""",
        {
            "uuid": uuid_new,
            "sid": sid_new,
            "title": title,
            "descr": descr,
            "task": task,
        },
    )
    res = tpl.TemplateResponse(
        "subtasks/view.htm",
        {
            "request": req,
            "subtask": dict(subt._mapping)
            | {"author_email_hash": user["email_hash"]},
            "user": user,
        },
    )
    res.headers["HX-Trigger"] = "subtask-create"
    return res


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
        UPDATE subtasks
        SET complete = :complete, title = :title, descr = :descr
        WHERE sid = :sid AND task IN (
            SELECT uuid FROM tasks WHERE section IN (
                SELECT uuid FROM sections WHERE creator = :user
            )
        )
        RETURNING *
        """,
        {
            "sid": sid,
            "complete": 1 if complete else 0,
            "title": title,
            "descr": descr,
            "user": user['uuid'],
        },
    )
    res = tpl.TemplateResponse(
        "subtasks/view.htm",
        {"request": req, "subtask": task},
    )
    res.headers["HX-Trigger"] = "subtask-update"
    if task.complete == 1:
        res.headers["HX-Trigger"] = "subtask-complete"
    else:
        res.headers["HX-Trigger"] = "subtask-incomplete"
    return res


