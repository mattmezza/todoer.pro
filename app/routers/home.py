from typing import Mapping

from databases import Database
from fastapi import APIRouter, Depends, Request
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates

from app import templating
from app.dependencies import (
    Renderer,
    get_current_active_user,
    get_db,
    get_tpl,
    renderer,
)

router = APIRouter()


@router.get("/")
async def home(
    req: Request,
    tpl: Jinja2Templates = Depends(get_tpl),
    db: Database = Depends(get_db),
    user: Mapping = Depends(get_current_active_user),
):
    spaces = await db.fetch_all(
        """
        SELECT * FROM spaces
        WHERE creator = :creator
        ORDER BY creator
        LIMIT 100
        """,
        {"creator": user["uuid"]},
    )
    lists = await db.fetch_all("select title, uuid from lists limit 50")
    return tpl.TemplateResponse(
        "pages/home.htm",
        templating.constants
        | {
            "request": req,
            "spaces": [sp._mapping for sp in spaces],
            "lists": [li._mapping for li in lists],
            "user": user,
        },
    )


@router.get("/dashboard")
async def temp(
    req: Request,
    render: Renderer = Depends(renderer),
    db: Database = Depends(get_db),
    user: Mapping = Depends(get_current_active_user),
):
    spaces = await db.fetch_all(
        """
        SELECT * FROM spaces
        WHERE creator = :creator
        ORDER BY creator
        LIMIT 100
        """,
        {"creator": user["uuid"]},
    )
    lists = await db.fetch_all("select title, uuid from lists limit 50")
    return HTMLResponse(
        content=render(
            "Page.Dashboard",
            request=req,
            spaces=spaces,
            lists=lists,
            user=user,
        )
    )
