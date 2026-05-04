import csv
import io
from typing import Annotated, Mapping
from uuid import UUID

from fastapi import APIRouter, Depends, Form, Query, Request
from fastapi.responses import HTMLResponse, PlainTextResponse, Response

from app import http, svc, tpl
from app.deps import Renderer, get_current_active_user, renderer, repo_db
from app.repo import RepoDB
from app.utils import slugify_title

router = APIRouter()


@router.patch("/list/{sid}/archive", response_class=HTMLResponse)
async def archive(
    req: Request,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    lrepo = urepo.lists()
    list_ = http.Opt(await lrepo.by_sid(sid)).or_404(f"List #{sid} not found.")
    space = await urepo.spaces().by_uuid(list_["space"])
    await lrepo.archive(sid)
    return HTMLResponse(
        "",
        headers={"HX-Redirect": f"/space/{space['sid']}"},
    )


@router.delete("/list/{sid}", response_class=HTMLResponse)
async def delete(
    req: Request,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    lrepo = urepo.lists()
    list_ = http.Opt(await lrepo.by_sid(sid)).or_404(f"List #{sid} not found.")
    space = await urepo.spaces().by_uuid(list_["space"])
    await lrepo.del_by_sid(sid)
    res = HTMLResponse("")
    res.headers["HX-Trigger"] = f"list-delete, list-{sid}-delete"
    res.headers["HX-Redirect"] = f"/space/{space['sid']}"
    return res


@router.get("/list/{sid}")
@tpl.htmx("Page.List")
async def view(
    req: Request,
    sid: str,
    view_complete: bool | None = None,
    sort_by: str | None = None,
    filter_priority: str | None = Query(None, alias="priority"),
    filter_due: str | None = Query(None, alias="due"),
    filter_tags: str | None = Query(None, alias="tags"),
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
    render: Renderer = Depends(renderer),
):
    user_repo = db.for_user(user)
    list_r = user_repo.lists()
    li = http.Opt(await list_r.by_sid(sid)).or_404(f"List #{sid} not found.")

    # Use list setting as default, allow query param override
    if view_complete is None:
        view_complete = bool(li.get("show_completed", 0))

    if sort_by is None:
        sort_by = li.get("sort_by", "manual")

    # Parse filter params
    filter_priority_val = int(filter_priority) if filter_priority else None
    filter_tags_list = [t.strip() for t in filter_tags.split(",") if t.strip()] if filter_tags else None

    space_r = user_repo.spaces()
    section_r = user_repo.sections()
    space = await space_r.by_uuid(li["space"])
    sections = await section_r.by_list(li["uuid"])
    tarepo = user_repo.tasks()

    # Use filtered query if any filter is active
    has_filters = filter_priority_val is not None or filter_due or filter_tags_list
    if has_filters:
        tasks_by_section = defaultdict(list)
        filtered = await tarepo.filtered_by_list(
            li["uuid"],
            sort_by=sort_by,
            priority=filter_priority_val,
            due=filter_due,
            tags=filter_tags_list,
        )
        for t in filtered:
            tasks_by_section[t["section"]].append(t)
    else:
        tasks_by_section = await tarepo.by_section_for_list(li["uuid"])
    for tt in tasks_by_section.values():
        svc.lists.sort_tasks(tt, sort_by)
        incomplete = [i for i, t in enumerate(tt) if not t["complete"]]
        for idx, ta in enumerate(tt):
            tt[idx] = dict(ta)
            if not tt[idx]["complete"] and incomplete:
                tt[idx]["is_first"] = idx == incomplete[0]
                tt[idx]["is_last"] = idx == incomplete[-1]
            else:
                tt[idx]["is_first"] = True
                tt[idx]["is_last"] = True
    all_sections = await section_r.all_by_space(space["uuid"])
    sections_by_list = section_r.group_by_list(all_sections)
    is_bookmarked = await user_repo.bookmarks().is_bookmarked(
        "list", li["uuid"]
    )
    all_tags = await tarepo.all_tags_annotated()
    return {
        "spaces": await space_r.all(),
        "space": space,
        "lists": await list_r.all_by_space(space["sid"]),
        "list": li,
        "sections": sections,
        "sections_by_list": sections_by_list,
        "view_complete": view_complete,
        "is_bookmarked": is_bookmarked,
        "user": user,
        "tasks_by_section": tasks_by_section,
        "sort_by": sort_by,
        "filter_priority": filter_priority_val,
        "filter_due": filter_due,
        "filter_tags": filter_tags_list,
        "all_tags": all_tags,
    }


@router.get("/list/{sid}/export.txt")
async def export_txt(
    req: Request,
    sid: str,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    urepo = db.for_user(user)
    li = http.Opt(await urepo.lists().by_sid(sid)).or_404()
    sections = await urepo.sections().by_list(li["uuid"])
    tasks_by_section = await urepo.tasks().by_section_for_list(li["uuid"])

    lines = [f"# {li['title']}", ""]
    if li.get("descr"):
        lines.append(li["descr"])
        lines.append("")

    for section in sections:
        tasks = tasks_by_section.get(section["uuid"], [])
        if not section["default"]:
            lines.append(f"## {section['title']}")
        for task in tasks:
            status = "[x]" if task["complete"] else "[ ]"
            line = f"  {status} {task['title']}"
            if task.get("due_date"):
                line += f" (due: {task['due_date']})"
            lines.append(line)
        lines.append("")

    content = "\n".join(lines)
    return PlainTextResponse(
        content,
        headers={
            "Content-Disposition": (
                f'attachment; filename="{li["title"]}.txt"'
            )
        },
    )


@router.get("/list/{sid}/export.csv")
async def export_csv(
    req: Request,
    sid: str,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    urepo = db.for_user(user)
    li = http.Opt(await urepo.lists().by_sid(sid)).or_404()
    sections = await urepo.sections().by_list(li["uuid"])
    tasks_by_section = await urepo.tasks().by_section_for_list(li["uuid"])

    output = io.StringIO()
    writer = csv.writer(output)
    writer.writerow(["Section", "Task", "Status", "Due Date", "Created"])

    for section in sections:
        tasks = tasks_by_section.get(section["uuid"], [])
        section_title = (
            section["title"] if not section["default"] else "(default)"
        )
        for task in tasks:
            writer.writerow(
                [
                    section_title,
                    task["title"],
                    "Complete" if task["complete"] else "Incomplete",
                    task.get("due_date", ""),
                    task.get("created", ""),
                ]
            )

    return Response(
        content=output.getvalue(),
        media_type="text/csv",
        headers={
            "Content-Disposition": (
                f'attachment; filename="{li["title"]}.csv"'
            )
        },
    )


@router.put("/list/{sid}/color")
@tpl.htmx("Lists.Color")
async def update_color(
    req: Request,
    res: HTMLResponse,
    sid: str,
    color: Annotated[str, Form()],
    user: Mapping = Depends(get_current_active_user),
    render: Renderer = Depends(renderer),
    db: RepoDB = Depends(repo_db),
):
    col = http.Opt(
        await db.for_user(user).lists().update_color_by_sid(color.strip(), sid)
    ).or_404(f"List #{sid} not found.")
    return (
        dict(color=col),
        200,
        {"HX-Trigger": f"list-update, list-{sid}-update"},
    )


@router.get("/list/{sid}/settings")
@tpl.htmx("Lists.Settings")
async def get_settings(
    req: Request,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    li = http.Opt(
        await db.for_user(user).lists().by_sid(sid)
    ).or_404(f"List #{sid} not found.")
    return {"list": li}


@router.put("/list/{sid}/settings")
@tpl.htmx("Lists.Settings")
async def update_settings(
    req: Request,
    sid: str,
    default_view: Annotated[str, Form()] = "board",
    show_completed: Annotated[bool, Form()] = False,
    sort_by: Annotated[str, Form()] = "manual",
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    li = http.Opt(
        await db.for_user(user)
        .lists()
        .update_settings_by_sid(
            sid,
            default_view=default_view,
            show_completed=show_completed,
            sort_by=sort_by,
        )
    ).or_404(f"List #{sid} not found.")
    return (
        {"list": li},
        200,
        {"HX-Trigger": f"list-update, list-{sid}-update"},
    )


@router.put("/list/{sid}/title")
async def update_title(
    req: Request,
    res: HTMLResponse,
    sid: str,
    title: Annotated[str, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    t = http.Opt(
        await db.for_user(user).lists().update_title_by_sid(slugify_title(title), sid)
    ).or_404(f"List #{sid} not found.")
    res = PlainTextResponse("#" + t)
    res.headers["HX-Trigger"] = f"list-update, list-{sid}-update"
    return res


@router.put("/list/{sid}/descr", response_class=HTMLResponse)
@tpl.htmx("Lists.Descr")
async def update_descr(
    req: Request,
    res: HTMLResponse,
    sid: str,
    descr: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    li = http.Opt(
        await db.for_user(user)
        .lists()
        .update_descr_by_sid(descr.strip() if descr else None, sid)
    ).or_404(f"List #{sid} not found.")
    res.headers["HX-Trigger"] = f"list-update, list-{sid}-update"
    return {"list": li}


@router.put("/list/{sid}/space")
async def update_space(
    req: Request,
    res: HTMLResponse,
    sid: str,
    space: Annotated[UUID, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    _ = http.Opt(
        await db.for_user(user).lists().update_space_by_sid(space, sid)
    ).or_404(f"List #{sid} not found.")
    res = PlainTextResponse()
    res.headers["HX-Redirect"] = f"/list/{sid}"
    return res


@router.post("/lists")
@tpl.htmx("Nav.Lists.Item")
async def add(
    req: Request,
    space: Annotated[str, Form()],
    title: Annotated[str, Form()],
    descr: Annotated[str | None, Form()] = None,
    color: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    li, _ = await svc.lists.create(
        user,
        db,
        color if color else svc.lists.DEFAULT_COLOR,
        slugify_title(title),
        descr.strip() if descr else None,
        space,
        default=False,
    )
    return (
        {"list": li, "user": user},
        201,
        {"HX-Trigger": "list-create", "HX-Redirect": f"/list/{li['sid']}"},
    )
