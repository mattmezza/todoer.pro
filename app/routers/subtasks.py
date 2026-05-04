from typing import Annotated, Mapping

from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse

from app import blob, http, tpl
from app.deps import RepoDB, get_current_active_user, repo_db

router = APIRouter()


@router.get("/subtask/{sid}")
@tpl.htmx("Page.Subtask")
async def view(
    req: Request,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    full = http.Opt(await urepo.subtasks().by_sid_full(sid)).or_404(
        f"Subtask #{sid} not found."
    )

    subtask = await urepo.subtasks().by_sid(sid)
    children = await urepo.subtasks().by_parent(subtask["uuid"])
    ancestors = await urepo.subtasks().ancestors(subtask["uuid"])

    task = {
        "uuid": full["task_uuid"],
        "sid": full["task_sid"],
        "title": full["task_title"],
        "due_date": full.get("task_due_date"),
        "priority": full.get("task_priority"),
        "tags": full.get("task_tags"),
        "complete": full.get("task_complete", 0),
        "completed": full.get("task_completed"),
    }
    section = {
        "uuid": full["section_uuid"],
        "sid": full["section_sid"],
        "title": full["section_title"],
    }
    list_ = {
        "uuid": full["list_uuid"],
        "sid": full["list_sid"],
        "title": full["list_title"],
        "color": full["list_color"],
        "default": full["list_default"],
    }
    space = {
        "uuid": full["space_uuid"],
        "sid": full["space_sid"],
        "title": full["space_title"],
        "default": full["space_default"],
    }

    task_id = task["uuid"]
    comments = await urepo.comments().by_task(task_id)
    attachments = await urepo.attachments().by_task(task_id)
    attachments = [dict(at) for at in attachments]
    for a in attachments:
        a['url'] = blob.url_signed_for(
            a['uuid'],
            blob.URL_SIGNED_VALIDITY_DURATION_DEFAULT,
        )
    audit_log = await urepo.tasks().audit_log(task_id)

    srepo = urepo.spaces()
    lrepo = urepo.lists()
    spaces = await srepo.all()
    lists = await lrepo.all_by_space(space["sid"])

    return {
        "user": user,
        "subtask": subtask,
        "children": children,
        "ancestors": ancestors,
        "task": task,
        "section": section,
        "list": list_,
        "lists": lists,
        "space": space,
        "spaces": spaces,
        "comments": comments,
        "attachments": attachments,
        "audit_log": audit_log,
    }


@router.delete("/subtask/{sid}", response_class=HTMLResponse)
async def delete(
    req: Request,
    res: HTMLResponse,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    await db.for_user(user).subtasks().del_by_sid(sid)
    res.headers["HX-Trigger"] = f"task-update, subtask-delete, subtask-{sid}-delete"
    return ""


@router.post("/subtasks")
@tpl.htmx("Subtask")
async def add(
    req: Request,
    task: Annotated[str, Form()],
    title: Annotated[str, Form()],
    descr: Annotated[str | None, Form()] = None,
    parent: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    subtask = http.Opt(
        await db.for_user(user)
        .subtasks()
        .create(
            title.strip(),
            descr.strip() if descr else None,
            task,
            parent or None,
        )
    ).or_400()
    return (
        {
            "subtask": dict(subtask)
            | {"author_email_hash": user["email_hash"]},
        },
        201,
        {"HX-Trigger": "task-update, subtask-create"},
    )


@router.put("/subtask/{sid}")
@tpl.htmx("Subtask")
async def update(
    req: Request,
    sid: str,
    title: Annotated[str, Form()],
    descr: Annotated[str | None, Form()] = None,
    complete: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    subtask = http.Opt(
        await db.for_user(user)
        .subtasks()
        .update(sid, title.strip(), descr.strip() if descr else None, complete)
    ).or_400()
    hx_trigger = f"task-update, subtask-update, subtask-{sid}-update"
    if subtask["complete"] == 1:
        hx_trigger += f", subtask-complete, subtask-{sid}-complete"
    else:
        hx_trigger += f", subtask-incomplete, subtask-{sid}-incomplete"
    return {"subtask": subtask}, 200, {"HX-Trigger": hx_trigger}


@router.put("/subtask/{sid}/title")
@tpl.htmx("Subtasks.Title")
async def update_title(
    req: Request,
    sid: str,
    title: Annotated[str, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    subtask = http.Opt(
        await db.for_user(user).subtasks().update_title(sid, title.strip())
    ).or_404(f"Subtask #{sid} not found.")
    return (
        {"subtask": subtask},
        200,
        {"HX-Trigger": f"task-update, subtask-update, subtask-{sid}-update"},
    )


@router.put("/subtask/{sid}/descr", response_class=HTMLResponse)
@tpl.htmx("Subtasks.Descr")
async def update_descr(
    req: Request,
    res: HTMLResponse,
    sid: str,
    descr: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    subtask = http.Opt(
        await db.for_user(user)
        .subtasks()
        .update_descr(sid, descr.strip() if descr else None)
    ).or_404(f"Subtask #{sid} not found.")
    res.headers["HX-Trigger"] = f"task-update, subtask-update, subtask-{sid}-update"
    return {"subtask": subtask}


@router.put("/subtask/{sid}/complete")
@tpl.htmx("Subtasks.Title")
async def update_complete(
    req: Request,
    sid: str,
    complete: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    subtask = http.Opt(
        await db.for_user(user).subtasks().update_complete(sid, bool(complete))
    ).or_404(f"Subtask #{sid} not found.")
    return (
        {"subtask": subtask},
        200,
        {"HX-Trigger": f"task-update, subtask-update, subtask-{sid}-update"},
    )


@router.put("/subtask/{sid}/priority")
@tpl.htmx("Tasks.Priority")
async def update_subtask_priority(
    req: Request,
    sid: str,
    priority: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    priority_val = int(priority) if priority else None
    if priority_val is not None and priority_val not in (1, 2, 3, 4):
        priority_val = None
    subtask = http.Opt(
        await urepo.subtasks().update_priority_by_sid(priority_val, sid)
    ).or_404(f"Subtask #{sid} not found.")
    return (
        {"task": subtask},
        200,
        {"HX-Trigger": f"subtask-update, subtask-{sid}-update"},
    )


@router.put("/subtask/{sid}/due-date")
@tpl.htmx("Tasks.DueDate")
async def update_subtask_due_date(
    req: Request,
    sid: str,
    due_date: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    subtask = http.Opt(
        await urepo.subtasks().update_due_date_by_sid(
            due_date if due_date else None, sid
        )
    ).or_404(f"Subtask #{sid} not found.")
    return (
        {"task": subtask},
        200,
        {"HX-Trigger": f"subtask-update, subtask-{sid}-update"},
    )


@router.put("/subtask/{sid}/tags")
@tpl.htmx("Tasks.Tags")
async def update_subtask_tags(
    req: Request,
    sid: str,
    tags: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    subtask = http.Opt(
        await urepo.subtasks().update_tags_by_sid(
            tags.strip() if tags else "", sid
        )
    ).or_404(f"Subtask #{sid} not found.")
    return (
        {"task": subtask},
        200,
        {"HX-Trigger": f"subtask-update, subtask-{sid}-update"},
    )
