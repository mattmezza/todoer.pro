from typing import Mapping

from fastapi import APIRouter, Depends, Request

from app import tpl
from app.deps import RepoDB, get_current_active_user, repo_db

router = APIRouter()


@router.get("/dashboard")
@tpl.htmx("Page.Dashboard")
async def dash(
    req: Request,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    urepo = db.for_user(user)
    stats = await urepo.tasks().stats()
    recent = await urepo.tasks().completed_per_period(30)
    return {
        "spaces": await urepo.spaces().all(),
        "user": user,
        "stats": stats,
        "recent_completions": recent,
    }
