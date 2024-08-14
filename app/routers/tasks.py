from typing import Annotated, Mapping
from uuid import UUID

from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse, PlainTextResponse

from app import blob, http, tpl
from app.deps import (
    RepoDB,
    get_current_active_user,
    get_db,
    repo_db,
)

router = APIRouter()


@router.post("/tasks")
@tpl.htmx("Tasks.TaskItem")
async def add_task(
    req: Request,
    title: Annotated[str, Form()],
    section: Annotated[str, Form()],
    descr: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    task = await urepo.tasks().create(
        title.strip(), descr.strip() if descr else None, section
    )
    section = await urepo.sections().by_uuid(section)
    return (
        {"task": task},
        201,
        {"HX-Trigger": f"task-create, section-{section.sid}-update"},
    )


@router.get("/task/{sid}")
@tpl.htmx("Page.Task")
async def view(
    req: Request,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
    abase=Depends(get_db),
):
    urepo = db.for_user(user)
    task = http.Opt(await urepo.tasks().by_sid(sid)).or_404(
        f"Task #{sid} not found."
    )
    secrepo = urepo.sections()
    section = await secrepo.by_uuid(task["section"])
    lrepo = urepo.lists()
    list_ = await lrepo.by_uuid(section["list"])
    sections = await secrepo.all_by_list(list_.uuid)
    srepo = urepo.spaces()
    space = await srepo.by_uuid(list_["space"])
    lists = await lrepo.all_by_space(space["sid"])
    task_id = task["uuid"]
    comments = await urepo.comments().by_task(task_id)
    subtasks = await urepo.subtasks().by_task(task_id)
    attachments = await urepo.attachments().by_task(task_id)
    attachments = [dict(at) for at in attachments]
    for a in attachments:
        a['url'] = blob.url_signed_for(
            a['uuid'],
            blob.URL_SIGNED_VALIDITY_DURATION_DEFAULT,
        )
    return {
        "task": task,
        "section": section,
        "sections": sections,
        "list": list_,
        "lists": lists,
        "space": space,
        "spaces": await srepo.all(),
        "comments": comments,
        "subtasks": subtasks,
        "attachments": attachments,
        "user": user,
    }


@router.put("/task/{sid}")
@tpl.htmx("Tasks.TaskItem")
async def update(
    req: Request,
    sid: str,
    title: Annotated[str, Form()],
    descr: Annotated[str | None, Form()] = None,
    complete: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    task = http.Opt(
        await db.for_user(user)
        .tasks()
        .update_task_by_sid(
            complete, title.strip(), descr.strip() if descr else None, sid
        )
    ).or_404(f"Task #{sid} not found.")
    hx_trigger = f"task-update, task-{sid}-update"
    hx_trigger += f", section-{task['section']}-update"
    if task.complete == 1:
        hx_trigger += f", task-complete task-{sid}-complete"
    else:
        hx_trigger += f", task-incomplete, task-{sid}-incomplete"
    return {"task": task}, 200, {"HX-Trigger": hx_trigger}


@router.put("/task/{sid}/descr", response_class=HTMLResponse)
async def update_descr(
    req: Request,
    res: HTMLResponse,
    sid: str,
    descr: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    descr = (
        await db.for_user(user)
        .tasks()
        .update_descr_by_sid(descr.strip() if descr else None, sid)
    )
    res.headers["HX-Trigger"] = f"task-update, task-{sid}-update"
    return tpl.linkify_mentions(tpl.md(descr))


@router.put("/task/{sid}/section", response_class=PlainTextResponse)
async def update_section(
    req: Request,
    res: PlainTextResponse,
    sid: str,
    section: Annotated[UUID, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    _ = await db.for_user(user).tasks().update_section(section, sid)
    res.headers["HX-Redirect"] = f"/task/{sid}"
    return ""


@router.put("/task/{sid}/complete")
@tpl.htmx("Tasks.Title")
async def update_complete(
    req: Request,
    sid: str,
    complete: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    task = http.Opt(
        await urepo.tasks().update_completed_by_sid(complete, sid)
    ).or_404(f"Task #{sid} not found.")
    section = await urepo.sections().by_uuid(task["section"])
    list_ = await urepo.lists().by_uuid(section["list"])
    space = await urepo.spaces().by_uuid(list_["space"])
    return (
        {
            "task": task,
            "section": section,
            "list": list_,
            "space": space,
        },
        200,
        {"HX-Trigger": f"task-update, task-{sid}-update"},
    )


@router.put("/task/{sid}/title")
@tpl.htmx("Tasks.Title")
async def update_title(
    req: Request,
    sid: str,
    title: Annotated[str, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    task = http.Opt(
        await urepo.tasks().update_title_by_sid(title.strip(), sid)
    ).or_404(f"Task #{sid} not found.")
    section = await urepo.sections().by_uuid(task["section"])
    list_ = await urepo.lists().by_uuid(section["list"])
    space = await urepo.spaces().by_uuid(list_["space"])
    return (
        {
            "task": task,
            "section": section,
            "list": list_,
            "space": space,
        },
        200,
        {"HX-Trigger": f"task-update, task-{sid}-update"},
    )


@router.delete("/task/{sid}", response_class=HTMLResponse)
async def delete(
    req: Request,
    res: HTMLResponse,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    await db.for_user(user).tasks().del_by_sid(sid)
    res.headers["HX-Trigger"] = f"task-delete, task-{sid}-delete"
    return ""
