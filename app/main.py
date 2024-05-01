import os

import arel
from fastapi import Depends, FastAPI
from fastapi.staticfiles import StaticFiles

from .dependencies import get_current_active_user, tpl
from .routers import (
    attachments,
    auth,
    comments,
    home,
    lists,
    nav,
    sections,
    subtasks,
    tasks,
    users,
    spaces,
)

app = FastAPI()

if os.environ["TODOER_ENV"] == "local":
    hot_reload = arel.HotReload(
        paths=[
            arel.Path("app"),
            arel.Path("htm"),
            arel.Path("public"),
            arel.Path("styles"),
            arel.Path("tailwind.config.js"),
        ]
    )
    app.add_websocket_route("/hot-reload", route=hot_reload, name="hot-reload")
    app.add_event_handler("startup", hot_reload.startup)
    app.add_event_handler("shutdown", hot_reload.shutdown)
    tpl.env.globals["DEBUG"] = True
    tpl.env.globals["hot_reload"] = hot_reload

app.mount("/public", StaticFiles(directory="public"), name="public")

app.include_router(auth.router)
app.include_router(
    users.router,
    prefix="/users",
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    nav.router, prefix="/nav", dependencies=[Depends(get_current_active_user)]
)
app.include_router(
    lists.router,
    prefix="/lists",
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    sections.router,
    prefix="/sections",
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    tasks.router,
    prefix="/tasks",
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    comments.router,
    prefix="/comments",
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    subtasks.router,
    prefix="/subtasks",
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    attachments.router,
    prefix="/attachments",
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    home.router, dependencies=[Depends(get_current_active_user)]
)
app.include_router(
    spaces.router,
    prefix="/spaces",
    dependencies=[Depends(get_current_active_user)],
)
