import json
from typing import Annotated, Mapping

from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse

from app import tpl
from app.deps import RepoDB, get_current_active_user, repo_db

router = APIRouter()

AI_SETTING_KEYS = [
    "ai_api_key",
    "ai_model",
    "ai_max_tokens",
    "ai_temperature",
]

DEFAULT_KEYBINDINGS = {
    "quick_add": "n",
    "search": "Meta+k",
    "shortcuts_help": "?",
    "focus_sidebar": "Alt+h",
    "focus_main": "Alt+l",
    "search_nav_down": "ArrowDown",
    "search_nav_up": "ArrowUp",
}


@router.get("/settings")
@tpl.htmx("Page.Settings")
async def settings_page(
    req: Request,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    urepo = db.for_user(user)
    ai_settings = await urepo.settings().get_many(AI_SETTING_KEYS)
    return {
        "spaces": await urepo.spaces().all(),
        "user": user,
        "ai_settings": ai_settings,
    }


@router.put("/settings/ai")
async def update_ai_settings(
    req: Request,
    ai_api_key: Annotated[str | None, Form()] = None,
    ai_model: Annotated[str | None, Form()] = None,
    ai_max_tokens: Annotated[str | None, Form()] = None,
    ai_temperature: Annotated[str | None, Form()] = None,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    srepo = db.for_user(user).settings()
    pairs = {
        "ai_api_key": ai_api_key,
        "ai_model": ai_model,
        "ai_max_tokens": ai_max_tokens,
        "ai_temperature": ai_temperature,
    }
    for key, val in pairs.items():
        if val is not None and val.strip():
            await srepo.set(key, val.strip())
        else:
            await srepo.delete(key)
    return HTMLResponse(
        "",
        headers={"HX-Redirect": "/settings"},
    )


@router.get("/settings/keybindings")
@tpl.htmx("Page.SettingsKeybindings")
async def keybindings_page(
    req: Request,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    urepo = db.for_user(user)
    raw = await urepo.settings().get("keybindings")
    user_bindings = json.loads(raw) if raw else {}
    merged = {**DEFAULT_KEYBINDINGS, **user_bindings}
    return {
        "spaces": await urepo.spaces().all(),
        "user": user,
        "keybindings": merged,
        "defaults": DEFAULT_KEYBINDINGS,
    }


@router.put("/settings/keybindings")
async def update_keybindings(
    req: Request,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    form = await req.form()
    bindings = {}
    for key in DEFAULT_KEYBINDINGS:
        val = form.get(key, "").strip()
        if val and val != DEFAULT_KEYBINDINGS.get(key):
            bindings[key] = val
    srepo = db.for_user(user).settings()
    if bindings:
        await srepo.set("keybindings", json.dumps(bindings))
    else:
        await srepo.delete("keybindings")
    return HTMLResponse(
        "",
        headers={"HX-Redirect": "/settings/keybindings"},
    )


@router.post("/settings/keybindings/reset")
async def reset_keybindings(
    req: Request,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    await db.for_user(user).settings().delete("keybindings")
    return HTMLResponse(
        "",
        headers={"HX-Redirect": "/settings/keybindings"},
    )
