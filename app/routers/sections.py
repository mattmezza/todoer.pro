from typing import Mapping

from databases import Database
from fastapi import APIRouter, Depends, Request
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates

from app.dependencies import get_current_active_user, get_db, get_tpl

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
    res = HTMLResponse("", status_code=204)
    res.headers["HX-Trigger"] = "list-delete"
    return res


@router.get("/{sid}", response_class=HTMLResponse)
async def view(
    req: Request,
    sid: str,
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
    html = {
        "tasks": [task_tpl.render({"task": task._mapping}) for task in tasks]
    }
    return tpl.TemplateResponse(
        "sections/view.htm",
        {
            "request": req,
            "section": ss._mapping,
            "tasks": [t._mapping for t in tasks],
            "html": html,
            "user": user,
        },
    )
