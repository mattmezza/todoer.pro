import os

import arel
from fastapi import FastAPI, Depends
from fastapi.staticfiles import StaticFiles

from .dependencies import tpl, get_current_active_user
from .routers import auth, home, lists, nav, todos, users


app = FastAPI()

if os.environ["ENV"] == "dev":
    hot_reload = arel.HotReload(paths=[arel.Path(".")])
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
    todos.router,
    prefix="/todos",
    dependencies=[Depends(get_current_active_user)],
)
app.include_router(
    home.router, dependencies=[Depends(get_current_active_user)]
)
