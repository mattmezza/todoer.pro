from typing import Mapping

from fastapi import APIRouter, Depends

from app import tpl
from app.deps import RepoDB, get_current_active_user, repo_db

router = APIRouter()


@router.get("/dashboard")
@tpl.htmx("Page.Dashboard")
async def dash(
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    return {
        "spaces": await db.for_user(user).spaces().all(),
        "user": user,
    }
