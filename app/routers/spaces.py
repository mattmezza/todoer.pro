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
    ss = await db.fetch_all(
        """
        SELECT
           s.*,
            COUNT(l.uuid) AS lists
        FROM spaces s
        LEFT JOIN lists l ON l.space = s.uuid
        WHERE s.creator = :creator
        GROUP BY s.uuid
        ORDER BY s.created ASC
        LIMIT 100
        """,
        {"creator": user["uuid"]},
    )
    num_tasks = await db.fetch_all(
        """
        SELECT
            sp.uuid as space,
            count(t.uuid) as tasks_tot
        FROM tasks t
        JOIN sections se on se.uuid = t.section
        JOIN lists l on l.uuid = se.list
        JOIN spaces sp on sp.uuid = l.space
        WHERE sp.creator = :creator
        GROUP BY sp.uuid
        LIMIT 100
        """,
        {"creator": user["uuid"]},
    )
    tasks_by_space = defaultdict(lambda: 0)
    for num_task in num_tasks:
        tasks_by_space[num_task.space] = num_task.tasks_tot
    s_tpl = tpl.get_template("spaces/view.htm")
    return tpl.TemplateResponse(
        "pages/spaces.htm",
        templating.constants
        | {
            "request": req,
            "spaces": [s._mapping for s in ss],
            "tasks_by_space": tasks_by_space,
            "user": user,
            "tpls": {
                "spaces": {"view": s_tpl.render},
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
    sp = await db.fetch_one(
        """INSERT INTO spaces (
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
        "spaces/view.htm",
        {
            "request": req,
            "space": sp._mapping,
            "tasks_by_space": {sp.uuid: 0},
            "user": user,
        },
    )
    res.headers["HX-Trigger"] = "space-create"
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
        DELETE FROM spaces
            WHERE sid = :sid AND creator = :creator
        """,
        {"sid": sid, "creator": user["uuid"]},
    )
    res = HTMLResponse("")
    res.headers["HX-Trigger"] = "space-delete"
    return res
