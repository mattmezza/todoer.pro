from databases import Database
from fastapi import APIRouter, Depends, Request
from fastapi.templating import Jinja2Templates
from typing import Mapping

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
        JOIN lists l ON l.workspace = w.uuid
        WHERE w.creator = :creator
        GROUP BY w.uuid
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
    tasks_by_ws = {}
    for num_task in num_tasks:
        tasks_by_ws[num_task.workspace] = num_task.tasks_tot
    return tpl.TemplateResponse(
        "pages/workspaces.htm",
        templating.constants
        | {
            "request": req,
            "workspaces": [w._mapping for w in ws],
            "tasks_by_ws": tasks_by_ws,
            "user": user,
        },
    )
