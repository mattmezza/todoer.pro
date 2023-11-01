from typing import Mapping

from databases import Database
from fastapi import APIRouter, Depends, Request
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates

from app.dependencies import get_db, get_tpl, get_current_active_user

router = APIRouter()


@router.get("/lists", response_class=HTMLResponse)
async def nav_lists(
    req: Request,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    lists = await db.fetch_all(
#        """SELECT w.* FROM (
#            SELECT l.*, COUNT(t.uuid) as incomplete FROM lists l
#                CROSS JOIN sections s ON s.list = l.uuid
#                CROSS JOIN tasks t ON t.section = s.uuid WHERE t.complete = 0
#                    GROUP BY l.uuid
#            ) as w
#            UNION ALL
#            SELECT wo.* FROM (
#                SELECT ll.*, 0 AS incomplete FROM lists ll
#                    WHERE NOT EXISTS (
#                        SELECT 'x' FROM sections ss WHERE ss.list = ll.uuid
#                    )
#            ) as wo
#            ORDER BY w.incomplete DESC, w.created
#            ASC LIMIT 50"""
"""
select *, 0 as incomplete from lists where creator = :creator
""",
{"creator": user["uuid"]}
        )
    return tpl.TemplateResponse(
        "nav/lists.htm",
        {"request": req, "lists": [li._mapping for li in lists]},
    )
