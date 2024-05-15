from typing import Annotated, Mapping

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
    task = await db.for_user(user).tasks().create(title, descr, section)
    return {"task": task}, 201, {"HX-Trigger": "task-create"}


@router.get("/task/{sid}")
@tpl.htmx("Page.Task")
async def view(
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
    abase=Depends(get_db),
):
    urepo = db.for_user(user)
    task = http.Opt(await urepo.tasks().by_sid(sid)).or_404(
        f"Task #{sid} not found."
    )
    section = await urepo.sections().by_uuid(task["section"])
    lrepo = urepo.lists()
    list_ = await lrepo.by_uuid(section["list"])
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
        .update_task_by_sid(complete, title, descr, sid)
    ).or_404(f"Task #{sid} not found.")
    hx_trigger = f"task-update, task-{sid}-update"
    if task.complete == 1:
        hx_trigger += f", task-complete task-{sid}-complete"
    else:
        hx_trigger += f", task-incomplete, task-{sid}-incomplete"
    return {"task": task}, 200, {"HX-Trigger": hx_trigger}


@router.put("/task/{sid}/descr", response_class=PlainTextResponse)
async def update_descr(
    res: PlainTextResponse,
    sid: str,
    descr: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    descr = await db.for_user(user).tasks().update_descr_by_sid(descr, sid)
    res.headers["HX-Trigger"] = f"task-update, task-{sid}-update"
    return descr


@router.put("/task/{sid}/complete")
@tpl.htmx("Tasks.Title")
async def update_complete(
    sid: str,
    complete: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    task = http.Opt(
        urepo.tasks().update_completed_by_sid(complete, sid)
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
    sid: str,
    title: Annotated[str, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    task = http.Opt(
        await urepo.tasks().update_title_by_sid(title, sid)
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
    res: HTMLResponse,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    await db.for_user(user).tasks().del_by_sid(sid)
    res.headers["HX-Trigger"] = f"task-delete, task-{sid}-delete"
    return ""
