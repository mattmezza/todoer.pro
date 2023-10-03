from typing import Annotated, Mapping

from databases import Database
from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates

from app.dependencies import get_current_active_user, get_db, get_tpl
from app.ids import new_ids

router = APIRouter()


@router.post("/", response_class=HTMLResponse)
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
            "title": title,
            "descr": descr,
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
    task = await db.fetch_one(
        "SELECT * FROM tasks WHERE sid = :sid", {"sid": sid}
    )
    se = await db.fetch_one(
        "SELECT * FROM sections WHERE uuid = :uuid",
        {"uuid": task.section},
    )
    li = await db.fetch_one(
        "SELECT * FROM lists WHERE uuid = :uuid",
        {"uuid": se.list},
    )
    return tpl.TemplateResponse(
        "tasks/detail.htm",
        {
            "request": req,
            "task": task._mapping,
            "section": se._mapping,
            "list": li._mapping,
            "user": user,
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
    return row.title


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
