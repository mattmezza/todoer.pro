import logging
from typing import Annotated, Mapping

from databases import Database
from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse, PlainTextResponse

from app.dependencies import (
    Renderer, get_current_active_user, get_db, renderer
)

logger = logging.getLogger(__name__)
router = APIRouter()


@router.get("/account")
async def read(
    req: Request,
    render: Renderer = Depends(renderer),
    db: Database = Depends(get_db),
    user: Mapping = Depends(get_current_active_user),
):
    spaces = await db.fetch_all(
        """
        SELECT * FROM spaces
        WHERE creator = :creator
        LIMIT 100
        """,
        {"creator": user["uuid"]}
    )
    return HTMLResponse(
        render(
            "Page/Account",
            request=req,
            spaces=[ss._mapping for ss in spaces],
            user=user,
        )
    )


@router.put("/account/fullname", response_class=PlainTextResponse)
async def change_name(
    req: Request,
    name: Annotated[str, Form()],
    db: Database = Depends(get_db),
    user: Mapping = Depends(get_current_active_user),
):
    row = await db.fetch_one(
        "UPDATE users SET name = :name WHERE uuid = :uuid RETURNING name",
        {"name": name, "uuid": user["uuid"]},
    )
    return row.name
