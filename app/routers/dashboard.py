from typing import Mapping

from databases import Database
from fastapi import APIRouter, Depends, Request

from app import tpl
from app.dependencies import get_current_active_user, get_db

router = APIRouter()


@router.get("/dashboard")
@tpl.htmx("Page/Dashboard")
async def dash(
    req: Request,
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
    return dict(
        request=req,
        spaces=spaces,
        lists=lists,
        user=user,
    )
