import os
import time

from fastapi import Depends, FastAPI, Request
from fastapi.middleware.gzip import GZipMiddleware
from fastapi.responses import HTMLResponse
from fastapi.staticfiles import StaticFiles

from . import tpl
from .deps import get_current_active_user, hot_reload
from .routers import (
    account,
    ai,
    attachments,
    auth,
    bookmarks,
    comments,
    dashboard,
    date,
    lists,
    nav,
    notifications,
    priority,
    search,
    sections,
    settings,
    spaces,
    subtasks,
    tasks,
)

app = FastAPI()

app.add_websocket_route("/hot-reload", route=hot_reload, name="hot-reload")
app.add_event_handler("startup", hot_reload.startup)
app.add_event_handler("shutdown", hot_reload.shutdown)
app.mount("/public", StaticFiles(directory="public"), name="public")
app.add_middleware(GZipMiddleware, minimum_size=1000, compresslevel=5)


@app.get("/")
async def landing(req: Request):
    tpl._current_request.set(req)
    return HTMLResponse(tpl.jinx.render("Page.Landing"))


@app.exception_handler(404)
async def handle_404(req: Request, _):
    tpl._current_request.set(req)
    return HTMLResponse(tpl.jinx.render("Page.404"), 404)


@app.middleware("http")
async def add_process_time_header(req: Request, call_next):
    start_time = time.time()
    res = await call_next(req)
    elapsed = time.time() - start_time
    res.headers["X-Process-Time"] = str(elapsed)
    return res


@app.middleware("http")
async def add_cache_control_header(req: Request, call_next):
    res = await call_next(req)
    if req.url.path.startswith("/manifest.json"):
        res.headers["Cache-Control"] = "public, max-age=31536000"
    elif req.url.path.startswith("/public/"):
        res.headers["Cache-Control"] = "public, max-age=31536000"
    else:
        res.headers["Cache-Control"] = "no-store, no-cache, must-revalidate"
    return res


@app.get("/manifest.json")
async def manifest(v: str = ""):
    env = os.environ["TODOER_ENV"]
    return {
        "theme_color": "#000000",
        "background_color": "#000000",
        "icons": [
            {
                "purpose": "maskable",
                "sizes": "512x512",
                "src": "/public/icon512_maskable.png",
                "type": "image/png",
            },
            {
                "purpose": "any",
                "sizes": "512x512",
                "src": "/public/icon512_rounded.png",
                "type": "image/png",
            },
        ],
        "orientation": "any",
        "display": "standalone",
        "dir": "auto",
        "lang": "en-US",
        "name": "Todoer.pro",
        "short_name": "Todoer",
        "start_url": "dashboard",
        "scope": (
            "http://localhost:8000/"
            if env == "local"
            else (
                "https://dev.todoer.pro/"
                if env == "dev"
                else "https://todoer.pro/"
            )
        ),
        "id": "todoer.pro",
        "description": "Get things done, like a pro!",
    }


app.include_router(auth.router)
app.include_router(
    account.router,
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    nav.router, prefix="/nav", dependencies=[Depends(get_current_active_user)]
)
app.include_router(
    lists.router,
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    sections.router,
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    tasks.router,
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    comments.router,
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    subtasks.router,
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    attachments.router,
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    bookmarks.router,
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    dashboard.router, dependencies=[Depends(get_current_active_user)]
)
app.include_router(
    spaces.router,
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    search.router,
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    notifications.router,
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    ai.router,
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    settings.router,
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    priority.router,
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    date.router,
    dependencies=[Depends(get_current_active_user)],
)
