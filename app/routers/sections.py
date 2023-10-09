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
        """DELETE FROM sections
            WHERE sid = :sid AND list IN (
                select uuid from lists where creator = :creator
        )""",
        {"sid": sid, "creator": user["uuid"]},
    )
    res = HTMLResponse("")
    res.headers["HX-Trigger"] = "section-delete"
    return res


@router.get("/{sid}", response_class=HTMLResponse)
async def view(
    req: Request,
    sid: str,
    view_complete: bool = False,
    db: Database = Depends(get_db),
    user: Mapping = Depends(get_current_active_user),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    ss = await db.fetch_one(
        """
        SELECT * FROM sections
        WHERE sid = :sid
        ORDER BY `order` ASC, created ASC
        LIMIT 1
        """,
        {"sid": sid},
    )
    tasks = await db.fetch_all(
        """SELECT * FROM tasks WHERE section = :section
        ORDER BY `order` ASC, created ASC""",
        {"section": ss.uuid},
    )
    task_tpl = tpl.get_template("tasks/view.htm")
    return tpl.TemplateResponse(
        "sections/view.htm",
        {
            "request": req,
            "tpls": {"task": {"view": task_tpl.render}},
            "section": ss._mapping,
            "tasks": [t._mapping for t in tasks],
            "tasks_incomplete": [t._mapping for t in tasks if t.complete == 0],
            "tasks_complete": [t._mapping for t in tasks if t.complete == 1],
            "view_complete": view_complete,
            "user": user,
        },
    )


@router.post("", response_class=HTMLResponse)
async def add(
    req: Request,
    title: Annotated[str, Form()],
    list_id: Annotated[str, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    uuid_new, sid_new = new_ids()
    se = await db.fetch_one(
        """INSERT INTO sections (
            uuid,
            sid,
            `order`,
            title,
            creator,
            list
        )
        VALUES (
            :uuid,
            :sid,
            (SELECT MAX(`order`) + 1 FROM sections WHERE list = :list),
            :title,
            :creator,
            :list
        )
        RETURNING *""",
        {
            "uuid": uuid_new,
            "sid": sid_new,
            "title": title.strip(),
            "creator": user["uuid"],
            "list": list_id,
        },
    )
    res = tpl.TemplateResponse(
        "sections/view.htm",
        {
            "request": req,
            "section": se._mapping,
            "user": user,
            "tasks": [],
            "html": {"tasks": []},
        },
    )
    res.headers["HX-Trigger"] = "section-create"
    return res
