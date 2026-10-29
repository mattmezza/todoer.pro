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
    trepo = urepo.tasks()
    stats = await trepo.stats()
    recent = await trepo.completed_per_period(30)
    overdue = await trepo.overdue()
    due_today = await trepo.due_today()
    upcoming = await trepo.upcoming(7)
    return {
        "spaces": await urepo.spaces().all(),
        "user": user,
        "stats": stats,
        "recent_completions": recent,
        "overdue": overdue,
        "due_today": due_today,
        "upcoming": upcoming,
    }
