from typing import Annotated, Mapping

from databases import Database
from fastapi import APIRouter, Depends, Request, Query
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates

from app import templating
from app.dependencies import RepoDB, get_db, get_tpl, get_current_active_user, repo_db

router = APIRouter()


@router.get("/lists", response_class=HTMLResponse)
async def nav_lists(
    req: Request,
    space: Annotated[str, Query()],
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    lists = await db.fetch_all(
        """
        SELECT *, 0 as incomplete FROM lists
        WHERE creator = :creator
            AND space = :space
        """,
        {"creator": user["uuid"], "space": space}
    )
    return tpl.TemplateResponse(
        "nav/lists.htm",
        {"request": req, "lists": [li._mapping for li in lists]},
    )


@router.get("/spaces")
@templating.htmx("Nav/Spaces")
async def nav_spaces(
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    return {"spaces": await db.for_user(user).spaces().all()},
