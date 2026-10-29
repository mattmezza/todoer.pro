from collections import defaultdict
from typing import Annotated, Mapping

from fastapi import APIRouter, Depends, Form, Query, Request
from fastapi.responses import HTMLResponse

from app import http, svc, tpl
from app.deps import Renderer, get_current_active_user, renderer, repo_db
from app.repo import RepoDB
from app.utils import slugify_title

router = APIRouter()


@router.get("/list/{sid}/board", response_class=HTMLResponse)
async def view_board(
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
    space_r = user_repo.spaces()
    section_r = user_repo.sections()
    space = await space_r.by_uuid(li["space"])
    sections = await section_r.by_list(li["uuid"])
    tarepo = user_repo.tasks()

    tasks_by_section = await tarepo.by_section_for_list(li["uuid"])
    for tt in tasks_by_section.values():
        svc.lists.sort_tasks(tt, sort_by)
        for idx, ta in enumerate(tt):
            tt[idx] = dict(ta)
    all_sections = await section_r.all_by_space(space["uuid"])
    sections_by_list = section_r.group_by_list(all_sections)
    is_bookmarked = await user_repo.bookmarks().is_bookmarked(
        "list", li["uuid"]
    )
    all_tags = await tarepo.all_tags_annotated()
    import datetime
    data = dict(
        spaces=await space_r.all(),
        space=space,
        lists=await list_r.all_by_space(space["sid"]),
        list=li,
        sections=sections,
        sections_by_list=sections_by_list,
        view_complete=False if view_complete is None else view_complete,
        is_bookmarked=is_bookmarked,
        user=user,
        tasks_by_section=tasks_by_section,
        sort_by=sort_by or "manual",
        filter_priority=int(filter_priority) if filter_priority else None,
        filter_due=filter_due,
        filter_tags=[t.strip() for t in filter_tags.split(",") if t.strip()] if filter_tags else None,
        all_tags=all_tags,
        today=datetime.date.today().isoformat(),
    )
    is_htmx = req.headers.get("hx-request") == "true"
    tpl_name = "Page.BoardContent" if is_htmx else "Page.Board"
    tpl._current_request.set(req)
    return HTMLResponse(render(tpl_name, **data))
