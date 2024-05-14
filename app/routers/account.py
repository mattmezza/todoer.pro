import logging
from typing import Annotated, Mapping

from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import PlainTextResponse

from app import tpl
from app.deps import RepoDB, get_current_active_user, repo_db

logger = logging.getLogger(__name__)
router = APIRouter()


@router.get("/account")
@tpl.htmx("Page.Account")
async def read(
    req: Request,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    return {
        "spaces": await db.for_user(user).spaces().all(),
        "user": user,
    }


@router.put("/account/fullname", response_class=PlainTextResponse)
async def change_name(
    res: PlainTextResponse,
    name: Annotated[str, Form()],
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    user = (
        await db.for_user(user)
        .users()
        .update_fname_by_uuid(name, user["uuid"])
    )
    res.headers["HX-Trigger"] = "account-update"
    return user["name"]
