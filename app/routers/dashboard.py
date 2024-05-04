from typing import Mapping

from databases import Database
from fastapi import APIRouter, Depends, Request
from fastapi.responses import HTMLResponse

from app.dependencies import (
    Renderer,
    get_current_active_user,
    get_db,
    renderer,
)

router = APIRouter()


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
