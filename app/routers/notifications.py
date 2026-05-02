from typing import Mapping

from fastapi import APIRouter, Depends, Form, Request

from app import tpl
from app.deps import RepoDB, get_current_active_user, repo_db

router = APIRouter()


async def _fetch_notifications(urepo, upcoming_days: int = 7):
    trepo = urepo.tasks()
    overdue = await trepo.overdue()
    due_today = await trepo.due_today()
    upcoming = await trepo.upcoming(upcoming_days)
    return overdue, due_today, upcoming


def _all_items(overdue, due_today, upcoming):
    items = []
    for t in overdue:
        items.append({"task_uuid": t["uuid"], "kind": "overdue"})
    for t in due_today:
        items.append({"task_uuid": t["uuid"], "kind": "today"})
    for t in upcoming:
        items.append({"task_uuid": t["uuid"], "kind": "upcoming"})
    return items


def _all_task_uuids(overdue, due_today, upcoming):
    return [t["uuid"] for t in overdue + due_today + upcoming]


@router.get("/notifications")
@tpl.htmx("Notifications.Panel")
async def notifications(
    req: Request,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    urepo = db.for_user(user)
    overdue, due_today, upcoming = await _fetch_notifications(urepo)
    nrepo = urepo.notification_reads()
    task_uuids = _all_task_uuids(overdue, due_today, upcoming)
    read_keys = await nrepo.get_read_keys(
        task_uuids, ["overdue", "today", "upcoming"]
    )
    return {
        "overdue": overdue,
        "due_today": due_today,
        "upcoming": upcoming,
        "read_tasks": read_keys,
    }


@router.get("/notifications/badge")
@tpl.htmx("Notifications.Badge")
async def notifications_badge(
    req: Request,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    urepo = db.for_user(user)
    overdue, due_today, upcoming = await _fetch_notifications(urepo)
    nrepo = urepo.notification_reads()
    task_uuids = _all_task_uuids(overdue, due_today, upcoming)
    read_keys = await nrepo.get_read_keys(
        task_uuids, ["overdue", "today", "upcoming"]
    )
    count = 0
    for t in overdue:
        if (t["uuid"], "overdue") not in read_keys:
            count += 1
    for t in due_today:
        if (t["uuid"], "today") not in read_keys:
            count += 1
    return {"count": count}


@router.get("/notifications/all")
@tpl.htmx("Page.Notifications")
async def notifications_all(
    req: Request,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    urepo = db.for_user(user)
    overdue, due_today, upcoming = await _fetch_notifications(
        urepo, upcoming_days=30
    )
    nrepo = urepo.notification_reads()
    task_uuids = _all_task_uuids(overdue, due_today, upcoming)
    read_keys = await nrepo.get_read_keys(
        task_uuids, ["overdue", "today", "upcoming"]
    )
    return {
        "spaces": await urepo.spaces().all(),
        "user": user,
        "overdue": overdue,
        "due_today": due_today,
        "upcoming": upcoming,
        "read_tasks": read_keys,
    }


@router.get("/notifications/all/content")
@tpl.htmx("Notifications.PageContent")
async def notifications_all_content(
    req: Request,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    urepo = db.for_user(user)
    overdue, due_today, upcoming = await _fetch_notifications(
        urepo, upcoming_days=30
    )
    nrepo = urepo.notification_reads()
    task_uuids = _all_task_uuids(overdue, due_today, upcoming)
    read_keys = await nrepo.get_read_keys(
        task_uuids, ["overdue", "today", "upcoming"]
    )
    return {
        "overdue": overdue,
        "due_today": due_today,
        "upcoming": upcoming,
        "read_tasks": read_keys,
    }


@router.post("/notifications/read")
@tpl.htmx("Notifications.Item")
async def mark_read(
    req: Request,
    task_uuid: str = Form(...),
    kind: str = Form(...),
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    urepo = db.for_user(user)
    nrepo = urepo.notification_reads()
    await nrepo.mark_read(task_uuid, kind)
    trepo = urepo.tasks()
    task = await trepo.by_uuid(task_uuid)
    return (
        {"task": task, "kind": kind, "is_read": True},
        200,
        {"HX-Trigger": "notification-update"},
    )


@router.post("/notifications/unread")
@tpl.htmx("Notifications.Item")
async def mark_unread(
    req: Request,
    task_uuid: str = Form(...),
    kind: str = Form(...),
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    urepo = db.for_user(user)
    nrepo = urepo.notification_reads()
    await nrepo.mark_unread(task_uuid, kind)
    trepo = urepo.tasks()
    task = await trepo.by_uuid(task_uuid)
    return (
        {"task": task, "kind": kind, "is_read": False},
        200,
        {"HX-Trigger": "notification-update"},
    )


@router.post("/notifications/read-full")
@tpl.htmx("Notifications.FullItem")
async def mark_read_full(
    req: Request,
    task_uuid: str = Form(...),
    kind: str = Form(...),
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    urepo = db.for_user(user)
    nrepo = urepo.notification_reads()
    await nrepo.mark_read(task_uuid, kind)
    trepo = urepo.tasks()
    task = await trepo.by_uuid(task_uuid)
    return (
        {"task": task, "kind": kind, "is_read": True},
        200,
        {"HX-Trigger": "notification-update"},
    )


@router.post("/notifications/unread-full")
@tpl.htmx("Notifications.FullItem")
async def mark_unread_full(
    req: Request,
    task_uuid: str = Form(...),
    kind: str = Form(...),
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    urepo = db.for_user(user)
    nrepo = urepo.notification_reads()
    await nrepo.mark_unread(task_uuid, kind)
    trepo = urepo.tasks()
    task = await trepo.by_uuid(task_uuid)
    return (
        {"task": task, "kind": kind, "is_read": False},
        200,
        {"HX-Trigger": "notification-update"},
    )


@router.post("/notifications/read-all")
@tpl.htmx("Notifications.Panel")
async def mark_all_read(
    req: Request,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    urepo = db.for_user(user)
    overdue, due_today, upcoming = await _fetch_notifications(urepo)
    items = _all_items(overdue, due_today, upcoming)
    nrepo = urepo.notification_reads()
    await nrepo.mark_all_read(items)
    task_uuids = _all_task_uuids(overdue, due_today, upcoming)
    read_keys = await nrepo.get_read_keys(
        task_uuids, ["overdue", "today", "upcoming"]
    )
    return (
        {
            "overdue": overdue,
            "due_today": due_today,
            "upcoming": upcoming,
            "read_tasks": read_keys,
        },
        200,
        {"HX-Trigger": "notification-update"},
    )


@router.post("/notifications/unread-all")
@tpl.htmx("Notifications.Panel")
async def mark_all_unread(
    req: Request,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    urepo = db.for_user(user)
    nrepo = urepo.notification_reads()
    await nrepo.mark_all_unread()
    overdue, due_today, upcoming = await _fetch_notifications(urepo)
    return (
        {
            "overdue": overdue,
            "due_today": due_today,
            "upcoming": upcoming,
            "read_tasks": set(),
        },
        200,
        {"HX-Trigger": "notification-update"},
    )
