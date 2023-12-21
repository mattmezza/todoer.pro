import logging
from typing import Annotated, Mapping

from databases import Database
from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import PlainTextResponse
from fastapi.templating import Jinja2Templates

from app import templating
from app.dependencies import get_current_active_user, get_db, get_tpl

logger = logging.getLogger(__name__)
router = APIRouter()


@router.get("/me")
async def me(
    req: Request,
    tpl: Jinja2Templates = Depends(get_tpl),
    db: Database = Depends(get_db),
    user: Mapping = Depends(get_current_active_user),
):
    lists = await db.fetch_all("SELECT title, sid FROM lists LIMIT 50")
    return tpl.TemplateResponse(
        "pages/user.htm",
        templating.constants
        | {
            "request": req,
            "lists": [li._mapping for li in lists],
            "user": user,
        },
    )


@router.put("/me/name", response_class=PlainTextResponse)
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
