from fastapi import Depends, FastAPI
from fastapi.staticfiles import StaticFiles

from .dependencies import get_current_active_user, hot_reload
from .routers import (
    account,
    attachments,
    auth,
    comments,
    dashboard,
    lists,
    nav,
    sections,
    spaces,
    subtasks,
    tasks,
)

app = FastAPI()

app.add_websocket_route("/hot-reload", route=hot_reload, name="hot-reload")
app.add_event_handler("startup", hot_reload.startup)
app.add_event_handler("shutdown", hot_reload.shutdown)
app.mount("/public", StaticFiles(directory="public"), name="public")

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
    dashboard.router, dependencies=[Depends(get_current_active_user)]
)
app.include_router(
    spaces.router,
    dependencies=[Depends(get_current_active_user)],
)
