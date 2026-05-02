import logging
from typing import Annotated, Mapping

from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse

from app import http, tpl
from app.deps import RepoDB, get_current_active_user, repo_db
from app.svc import ai as ai_svc

logger = logging.getLogger(__name__)

router = APIRouter()


@router.post("/ai/generate-tasks")
@tpl.htmx("Tasks.AiPreview")
async def generate_tasks(
    req: Request,
    prompt: Annotated[str, Form()],
    section_sid: Annotated[str, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    if not ai_svc.is_enabled():
        return (
            {"error": "AI feature is not available."},
            400,
        )

    urepo = db.for_user(user)

    # Rate limit check
    usage_count = await urepo.ai_usage().count_today()
    if usage_count >= ai_svc.MAX_GENERATIONS_PER_DAY:
        return (
            {
                "error": (
                    "Daily limit reached "
                    f"({ai_svc.MAX_GENERATIONS_PER_DAY} "
                    "generations per day). "
                    "Try again tomorrow."
                ),
            },
            429,
        )

    # Validate section exists and belongs to user
    section = http.Opt(await urepo.sections().by_sid(section_sid)).or_404(
        f"Section #{section_sid} not found."
    )

    try:
        tasks = await ai_svc.generate_tasks(prompt, section.get("title", ""))
    except ai_svc.AIInputError as e:
        return ({"error": str(e)}, 400)
    except ai_svc.AIError as e:
        logger.error("AI generation failed: %s", e)
        return ({"error": str(e)}, 500)

    # Log usage (audit trail — no full prompt/response)
    await urepo.ai_usage().log(
        prompt_length=len(prompt),
        tasks_generated=len(tasks),
    )

    return {
        "tasks": tasks,
        "section_sid": section_sid,
        "section": section,
    }


@router.post("/ai/add-tasks")
async def add_tasks(
    req: Request,
    section_sid: Annotated[str, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    if not ai_svc.is_enabled():
        return HTMLResponse("AI feature is not available.", 400)

    urepo = db.for_user(user)
    section = http.Opt(await urepo.sections().by_sid(section_sid)).or_404(
        f"Section #{section_sid} not found."
    )

    form = await req.form()
    task_indices = []
    for key in form.keys():
        if key.startswith("task_selected_"):
            idx = key.replace("task_selected_", "")
            if form[key] == "on":
                task_indices.append(idx)

    created_count = 0
    tarepo = urepo.tasks()
    for idx in task_indices:
        title = form.get(f"task_title_{idx}", "")
        descr = form.get(f"task_descr_{idx}", "")
        if not title or not str(title).strip():
            continue
        title = str(title).strip()[:100]
        descr = str(descr).strip()[:500] if descr else None
        await tarepo.create(title, descr, section["uuid"])
        created_count += 1

    hx_trigger = f"task-create, section-{section_sid}-update"
    return HTMLResponse(
        content="",
        status_code=200,
        headers={"HX-Trigger": hx_trigger},
    )
