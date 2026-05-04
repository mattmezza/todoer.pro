from typing import Annotated, Mapping

from fastapi import APIRouter, Depends, Query, Request

from app import tpl
from app.deps import RepoDB, get_current_active_user, repo_db

router = APIRouter()

PRIORITY_LABELS = {1: "P1", 2: "P2", 3: "P3", 4: "P4"}
PRIORITY_COLORS = {
    1: "red",
    2: "orange",
    3: "yellow",
    4: "blue",
}


@router.get("/priority")
@tpl.htmx("Page.PriorityHub")
async def hub(
    req: Request,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    urepo = db.for_user(user)
    counts = {}
    for p in [1, 2, 3, 4]:
        tasks = await urepo.tasks().all_by_priority(p, limit=500)
        counts[p] = len(tasks)
    return {
        "spaces": await urepo.spaces().all(),
        "user": user,
        "counts": counts,
    }


@router.get("/priority/{level}")
@tpl.htmx("Page.Priority")
async def view(
    req: Request,
    level: int,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    if level not in PRIORITY_LABELS:
        return "Invalid priority level", 404

    urepo = db.for_user(user)
    tasks = await urepo.tasks().all_by_priority(level)

    # Group tasks by space for display
    from collections import defaultdict

    by_space = defaultdict(list)
    for t in tasks:
        by_space[t["space_sid"]].append(t)

    return {
        "spaces": await urepo.spaces().all(),
        "user": user,
        "tasks": tasks,
        "by_space": dict(by_space),
        "priority_level": level,
        "priority_label": PRIORITY_LABELS[level],
        "priority_color": PRIORITY_COLORS[level],
        "tasks_total": len(tasks),
    }
