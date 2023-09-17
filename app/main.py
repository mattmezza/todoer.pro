import os

import arel
from fastapi import FastAPI
from fastapi.staticfiles import StaticFiles

from .dependencies import tpl
from .routers import home, lists, nav, todos


app = FastAPI()

if _debug := os.getenv("DEBUG"):
    hot_reload = arel.HotReload(paths=[arel.Path(".")])
    app.add_websocket_route("/hot-reload", route=hot_reload, name="hot-reload")
    app.add_event_handler("startup", hot_reload.startup)
    app.add_event_handler("shutdown", hot_reload.shutdown)
    tpl.env.globals["DEBUG"] = _debug
    tpl.env.globals["hot_reload"] = hot_reload

app.mount("/public", StaticFiles(directory="public"), name="public")

app.include_router(nav.router, prefix="/nav")
app.include_router(lists.router, prefix="/lists")
app.include_router(todos.router, prefix="/todos")
app.include_router(home.router)
