from typing import Annotated, Mapping
from uuid import UUID

from fastapi import APIRouter, Body, Depends, Form, Query, Request
from fastapi.responses import HTMLResponse, JSONResponse, PlainTextResponse

from app import blob, http, tpl
from app.deps import RepoDB, get_current_active_user, repo_db

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
    tarepo = urepo.tasks()
    task = await tarepo.create(
        title.strip(), descr.strip() if descr else None, section
    )
    await tarepo.log_action(task["uuid"], "created")
    task = dict(task)
    task["is_first"] = task["order"] == 0
    task["is_last"] = True
    secrepo = urepo.sections()
    section = await secrepo.by_uuid(section)
    sections = await secrepo.all_by_list(section["list"])
    list_ = await urepo.lists().by_uuid(section["list"])
    space = await urepo.spaces().by_uuid(list_["space"])
    all_sections = await secrepo.all_by_space(space["uuid"])
    sections_by_list = secrepo.group_by_list(all_sections)
    return (
        {
            "task": task,
            "section": section,
            "sections": sections,
            "sections_by_list": sections_by_list,
        },
        201,
        {"HX-Trigger": "task-create"},
    )


@router.put("/tasks/reorder", response_class=HTMLResponse)
async def reorder(
    req: Request,
    sids: Annotated[list[str], Body()],
    section_sid: Annotated[str, Body()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    await db.for_user(user).tasks().reorder(sids)
    return HTMLResponse(
        "",
        status_code=200,
        headers={
            "HX-Trigger": f"section-{section_sid}-update"
        },
    )


@router.post("/tasks/bulk/complete", response_class=HTMLResponse)
async def bulk_complete(
    req: Request,
    sids: Annotated[list[str], Body()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    count = await db.for_user(user).tasks().bulk_complete(sids)
    return HTMLResponse(
        "",
        status_code=200,
        headers={"HX-Trigger": "task-update, bulk-action-done"},
    )


@router.post("/tasks/bulk/delete", response_class=HTMLResponse)
async def bulk_delete(
    req: Request,
    sids: Annotated[list[str], Body()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    count = await db.for_user(user).tasks().bulk_delete(sids)
    return HTMLResponse(
        "",
        status_code=200,
        headers={"HX-Trigger": "task-delete, bulk-action-done"},
    )


@router.post("/tasks/bulk/move", response_class=HTMLResponse)
async def bulk_move(
    req: Request,
    sids: Annotated[list[str], Body()],
    section: Annotated[str, Body()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    count = await db.for_user(user).tasks().bulk_move(sids, section)
    return HTMLResponse(
        "",
        status_code=200,
        headers={
            "HX-Trigger": "section-update, bulk-action-done"
        },
    )


@router.get("/tasks/quick-add")
@tpl.htmx("Tasks.QuickAdd")
async def quick_add_form(
    req: Request,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    spaces = await urepo.spaces().all()
    default_space = await urepo.spaces().default()
    lists = await urepo.lists().all_by_space(default_space["sid"])
    default_list = await urepo.lists().default_for_space(
        default_space["sid"]
    )
    sections = await urepo.sections().by_list(default_list["uuid"])
    default_section = next(
        (s for s in sections if s.get("default")), sections[0]
    )
    return {
        "spaces": spaces,
        "lists": lists,
        "sections": sections,
        "default_space": default_space,
        "default_list": default_list,
        "default_section": default_section,
    }


@router.get("/tasks/quick-add/lists")
@tpl.htmx("Tasks.QuickAddLists")
async def quick_add_lists(
    req: Request,
    space: Annotated[str, Query()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    sp = http.Opt(await urepo.spaces().by_sid(space)).or_404(
        f"Space #{space} not found."
    )
    lists = await urepo.lists().all_by_space(sp["sid"])
    default_list = await urepo.lists().default_for_space(sp["sid"])
    sections = await urepo.sections().by_list(default_list["uuid"])
    default_section = next(
        (s for s in sections if s.get("default")), sections[0]
    )
    return {
        "lists": lists,
        "default_list": default_list,
        "sections": sections,
        "default_section": default_section,
    }


@router.get("/tasks/quick-add/sections")
@tpl.htmx("Tasks.QuickAddSections")
async def quick_add_sections(
    req: Request,
    list: Annotated[str, Query()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    li = http.Opt(await urepo.lists().by_uuid(list)).or_404(
        f"List #{list} not found."
    )
    sections = await urepo.sections().by_list(li["uuid"])
    default_section = next(
        (s for s in sections if s.get("default")), sections[0]
    )
    return {
        "sections": sections,
        "default_section": default_section,
    }


@router.get("/task/{sid}")
@tpl.htmx("Page.Task")
async def view(
    req: Request,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    tarepo = urepo.tasks()
    task = http.Opt(await tarepo.by_sid(sid)).or_404(f"Task #{sid} not found.")
    secrepo = urepo.sections()
    section = await secrepo.by_uuid(task["section"])
    lrepo = urepo.lists()
    list_ = await lrepo.by_uuid(section["list"])
    sections = await secrepo.all_by_list(list_.uuid)
    srepo = urepo.spaces()
    space = await srepo.by_uuid(list_["space"])
    lists = await lrepo.all_by_space(space["sid"])
    all_sections = await secrepo.all_by_space(space["uuid"])
    sections_by_list = secrepo.group_by_list(all_sections)
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
    audit_log = await tarepo.audit_log(task_id)
    return {
        "task": task,
        "section": section,
        "sections": sections,
        "sections_by_list": sections_by_list,
        "list": list_,
        "lists": lists,
        "space": space,
        "spaces": await srepo.all(),
        "comments": comments,
        "subtasks": subtasks,
        "attachments": attachments,
        "audit_log": audit_log,
        "user": user,
    }


@router.get("/task/{sid}/item")
@tpl.htmx("Tasks.TaskItem")
async def view_item(
    req: Request,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    task = dict(
        http.Opt(await urepo.tasks().by_sid(sid)).or_404(
            f"Task #{sid} not found."
        )
    )
    tasks = await urepo.tasks().by_section(task["section"])
    task = _add_first_last(task, tasks)
    secrepo = urepo.sections()
    section = await secrepo.by_uuid(task["section"])
    sections = await secrepo.all_by_list(section["list"])
    list_ = await urepo.lists().by_uuid(section["list"])
    space = await urepo.spaces().by_uuid(list_["space"])
    all_sections = await secrepo.all_by_space(space["uuid"])
    sections_by_list = secrepo.group_by_list(all_sections)
    return {
        "task": task,
        "sections": sections,
        "sections_by_list": sections_by_list,
        "section": section,
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
    urepo = db.for_user(user)
    tarepo = urepo.tasks()
    task = http.Opt(
        await tarepo.update_task_by_sid(
            complete, title.strip(), descr.strip() if descr else None, sid
        )
    ).or_404(f"Task #{sid} not found.")
    task = dict(task)
    tasks = await tarepo.by_section(task["section"])
    task = _add_first_last(task, tasks)
    secrepo = urepo.sections()
    section = await secrepo.by_uuid(task["section"])
    sections = await secrepo.all_by_list(section["list"])
    list_ = await urepo.lists().by_uuid(section["list"])
    space = await urepo.spaces().by_uuid(list_["space"])
    all_sections = await secrepo.all_by_space(space["uuid"])
    sections_by_list = secrepo.group_by_list(all_sections)
    hx_trigger = f"task-update, task-{sid}-update"
    hx_trigger += f", section-{task['section']}-update"
    if task["complete"] == 1:
        hx_trigger += f", task-complete, task-{sid}-complete"
    else:
        hx_trigger += f", task-incomplete, task-{sid}-incomplete"
    return (
        {
            "task": task,
            "sections": sections,
            "sections_by_list": sections_by_list,
            "section": section,
        },
        200,
        {"HX-Trigger": hx_trigger},
    )


@router.put("/task/{sid}/priority")
@tpl.htmx("Tasks.Priority")
async def update_priority(
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
    task = http.Opt(
        await urepo.tasks().update_priority_by_sid(priority_val, sid)
    ).or_404(f"Task #{sid} not found.")
    return (
        {"task": task},
        200,
        {"HX-Trigger": f"task-update, task-{sid}-update"},
    )


@router.put("/task/{sid}/due-date")
@tpl.htmx("Tasks.DueDate")
async def update_due_date(
    req: Request,
    sid: str,
    due_date: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    task = http.Opt(
        await urepo.tasks().update_due_date_by_sid(
            due_date if due_date else None, sid
        )
    ).or_404(f"Task #{sid} not found.")
    return (
        {"task": task},
        200,
        {"HX-Trigger": f"task-update, task-{sid}-update"},
    )


@router.put("/task/{sid}/tags")
@tpl.htmx("Tasks.Tags")
async def update_tags(
    req: Request,
    sid: str,
    tags: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    task = http.Opt(
        await urepo.tasks().update_tags_by_sid(
            tags.strip() if tags else "", sid
        )
    ).or_404(f"Task #{sid} not found.")
    return (
        {"task": task},
        200,
        {"HX-Trigger": f"task-update, task-{sid}-update"},
    )


@router.put("/task/{sid}/descr", response_class=HTMLResponse)
@tpl.htmx("Tasks.Descr")
async def update_descr(
    req: Request,
    res: HTMLResponse,
    sid: str,
    descr: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    tarepo = db.for_user(user).tasks()
    task = await tarepo.update_descr_by_sid(
        descr.strip() if descr else None, sid
    )
    task = dict(task)
    tasks = await tarepo.by_section(task["section"])
    task = _add_first_last(task, tasks)
    res.headers["HX-Trigger"] = f"task-update, task-{sid}-update"
    return {"task": task}


@router.put("/task/{sid}/section", response_class=HTMLResponse)
async def update_section(
    req: Request,
    sid: str,
    section: Annotated[UUID, Form()],
    reload: Annotated[bool, Query()] = False,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    async with db.transaction():
        task = http.Opt(
            await urepo.tasks().update_section(section, sid)
        ).or_404(f"Task #{sid} not found.")
        section = await urepo.sections().by_uuid(task["section"])
        headers = dict()
        headers["HX-Trigger"] = f"section-update, section-{section.sid}-update"
        if reload:
            headers["HX-Redirect"] = f"/task/{sid}"
        return HTMLResponse("", status_code=200, headers=headers)


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
    tarepo = urepo.tasks()
    task = http.Opt(
        await tarepo.update_completed_by_sid(complete, sid)
    ).or_404(f"Task #{sid} not found.")
    action = "completed" if task["complete"] else "uncompleted"
    await tarepo.log_action(task["uuid"], action)
    section = await urepo.sections().by_uuid(task["section"])
    list_ = await urepo.lists().by_uuid(section["list"])
    space = await urepo.spaces().by_uuid(list_["space"])
    tasks = await urepo.tasks().by_section(task["section"])
    task = dict(task)
    task = _add_first_last(task, tasks)
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
    tasks = await urepo.tasks().by_section(task["section"])
    task = dict(task)
    task = _add_first_last(task, tasks)
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


@router.patch("/task/{sid}/archive", response_class=HTMLResponse)
async def archive(
    req: Request,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    tarepo = urepo.tasks()
    task = await tarepo.archive(sid)
    if task:
        await tarepo.log_action(task["uuid"], "archived")
    return HTMLResponse(
        "",
        headers={"HX-Trigger": f"task-delete, task-{sid}-delete"},
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


@router.patch("/task/{sid}/move-up")
@tpl.htmx("Tasks.TaskItem")
async def move_up(
    req: Request,
    res: HTMLResponse,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    repo = db.for_user(user)
    tarepo = repo.tasks()
    task = http.Opt(await tarepo.move_up(sid)).or_404(
        f"Task #{sid} not found."
    )
    serepo = repo.sections()
    section = await serepo.by_uuid(task["section"])
    sections = await serepo.all_by_list(section["list"])
    sections_by_list = await _sections_by_list(repo, section)
    task = dict(task)
    tasks = await tarepo.by_section(task["section"])
    task = _add_first_last(task, tasks)
    return (
        {
            "task": task,
            "section": section,
            "sections": sections,
            "sections_by_list": sections_by_list,
        },
        200,
        {"HX-Trigger": f"section-{section.sid}-update"},
    )


@router.patch("/task/{sid}/move-down")
@tpl.htmx("Tasks.TaskItem")
async def move_down(
    req: Request,
    res: HTMLResponse,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    repo = db.for_user(user)
    tarepo = repo.tasks()
    task = http.Opt(await tarepo.move_down(sid)).or_404(
        f"Task #{sid} not found."
    )
    serepo = repo.sections()
    section = await serepo.by_uuid(task["section"])
    sections = await serepo.all_by_list(section["list"])
    sections_by_list = await _sections_by_list(repo, section)
    task = dict(task)
    tasks = await tarepo.by_section(task["section"])
    task = _add_first_last(task, tasks)
    return (
        {
            "task": task,
            "section": section,
            "sections": sections,
            "sections_by_list": sections_by_list,
        },
        200,
        {"HX-Trigger": f"section-{section.sid}-update"},
    )


@router.patch("/task/{sid}/move-top")
@tpl.htmx("Tasks.TaskItem")
async def move_top(
    req: Request,
    res: HTMLResponse,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    repo = db.for_user(user)
    tarepo = repo.tasks()
    task = http.Opt(await tarepo.by_sid(sid)).or_404(f"Task #{sid} not found.")
    if task.order == 0:
        return "", 204
    task = await tarepo.move_top(sid)
    serepo = repo.sections()
    section = await serepo.by_uuid(task["section"])
    sections = await serepo.all_by_list(section["list"])
    sections_by_list = await _sections_by_list(repo, section)
    task = dict(task)
    tasks = await tarepo.by_section(task["section"])
    task = _add_first_last(task, tasks)
    return (
        {
            "task": task,
            "section": section,
            "sections": sections,
            "sections_by_list": sections_by_list,
        },
        200,
        {"HX-Trigger": f"section-{section.sid}-update"},
    )


@router.patch("/task/{sid}/move-bottom")
@tpl.htmx("Tasks.TaskItem")
async def move_bottom(
    req: Request,
    res: HTMLResponse,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    repo = db.for_user(user)
    tarepo = repo.tasks()
    task = http.Opt(await tarepo.by_sid(sid)).or_404(f"Task #{sid} not found.")
    task = await tarepo.move_bottom(sid)
    serepo = repo.sections()
    section = await serepo.by_uuid(task["section"])
    sections = await serepo.all_by_list(section["list"])
    sections_by_list = await _sections_by_list(repo, section)
    task = dict(task)
    tasks = await tarepo.by_section(task["section"])
    task = _add_first_last(task, tasks)
    return (
        {
            "task": task,
            "section": section,
            "sections": sections,
            "sections_by_list": sections_by_list,
        },
        200,
        {"HX-Trigger": f"section-{section.sid}-update"},
    )


async def _sections_by_list(urepo, section):
    """Fetch all sections across all lists in the same space,
    grouped by list, for the cross-list move dialog."""
    secrepo = urepo.sections()
    list_ = await urepo.lists().by_uuid(section["list"])
    space = await urepo.spaces().by_uuid(list_["space"])
    all_sections = await secrepo.all_by_space(space["uuid"])
    return secrepo.group_by_list(all_sections)


def _add_first_last(task: dict, tasks: list[dict]):
    incomplete = [t for t in tasks if not t.get("complete")]
    if not incomplete or task.get("complete"):
        task["is_first"] = True
        task["is_last"] = True
    else:
        task["is_first"] = task["sid"] == incomplete[0]["sid"]
        task["is_last"] = task["sid"] == incomplete[-1]["sid"]
    return task
