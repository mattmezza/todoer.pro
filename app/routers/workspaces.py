from collections import defaultdict
from databases import Database
from fastapi import APIRouter, Depends, Request, Form
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates
from typing import Annotated, Mapping

from app.ids import new_ids
from app.dependencies import get_db, get_tpl, get_current_active_user
from app import templating

router = APIRouter()


@router.get("/")
async def index(
    req: Request,
    tpl: Jinja2Templates = Depends(get_tpl),
    db: Database = Depends(get_db),
    user: Mapping = Depends(get_current_active_user),
):
    ws = await db.fetch_all(
        """
        SELECT
            w.*,
            COUNT(l.uuid) AS lists
        FROM workspaces w
        LEFT JOIN lists l ON l.workspace = w.uuid
        WHERE w.creator = :creator
        GROUP BY w.uuid
        ORDER BY w.created ASC
        LIMIT 100
        """,
        {"creator": user["uuid"]},
    )
    num_tasks = await db.fetch_all(
        """
        SELECT
            w.uuid as workspace,
            count(t.uuid) as tasks_tot
        FROM tasks t
        JOIN sections s on s.uuid = t.section
        JOIN lists l on l.uuid = s.list
        JOIN workspaces w on w.uuid = l.workspace
        WHERE w.creator = :creator
        GROUP BY w.uuid
        LIMIT 100
        """,
        {"creator": user["uuid"]},
    )
    tasks_by_ws = defaultdict(lambda: 0)
    for num_task in num_tasks:
        tasks_by_ws[num_task.workspace] = num_task.tasks_tot
    ws_tpl = tpl.get_template("workspaces/view.htm")
    return tpl.TemplateResponse(
        "pages/workspaces.htm",
        templating.constants
        | {
            "request": req,
            "workspaces": [w._mapping for w in ws],
            "tasks_by_ws": tasks_by_ws,
            "user": user,
            "tpls": {
                "workspaces": {"view": ws_tpl.render},
            },
        },
    )


@router.post("", response_class=HTMLResponse)
async def add(
    req: Request,
    title: Annotated[str, Form()],
    descr: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    uuid_new, sid_new = new_ids()
    ws = await db.fetch_one(
        """INSERT INTO workspaces (
            uuid,
            sid,
            title,
            descr,
            creator
        )
        VALUES (
            :uuid,
            :sid,
            :title,
            :descr,
            :creator
        )
        RETURNING *""",
        {
            "uuid": uuid_new,
            "sid": sid_new,
            "title": title.strip(),
            "descr": descr.strip() if descr else None,
            "creator": user["uuid"],
        },
    )
    res = tpl.TemplateResponse(
        "workspaces/view.htm",
        {
            "request": req,
            "workspace": ws._mapping,
            "tasks_by_ws": {ws.uuid: 0},
            "user": user,
        },
    )
    res.headers["HX-Trigger"] = "workspace-create"
    return res


@router.delete("/{sid}", response_class=HTMLResponse)
async def delete(
    req: Request,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
):
    await db.execute(
        """
        DELETE FROM workspaces
            WHERE sid = :sid AND creator = :creator
        """,
        {"sid": sid, "creator": user["uuid"]},
    )
    res = HTMLResponse("")
    res.headers["HX-Trigger"] = "workspace-delete"
    return res
