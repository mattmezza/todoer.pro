from typing import Mapping

from fastapi import APIRouter, Depends, Request

from app import tpl
from app.deps import RepoDB, get_current_active_user, repo_db

router = APIRouter()


@router.get("/notifications")
@tpl.htmx("Notifications.Panel")
async def notifications(
    req: Request,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    urepo = db.for_user(user)
    trepo = urepo.tasks()
    overdue = await trepo.overdue()
    due_today = await trepo.due_today()
    upcoming = await trepo.upcoming(7)
    return {
        "overdue": overdue,
        "due_today": due_today,
        "upcoming": upcoming,
    }


@router.get("/notifications/badge")
@tpl.htmx("Notifications.Badge")
async def notifications_badge(
    req: Request,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    urepo = db.for_user(user)
    trepo = urepo.tasks()
    overdue = await trepo.overdue()
    due_today = await trepo.due_today()
    count = len(overdue) + len(due_today)
    return {"count": count}
