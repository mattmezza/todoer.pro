import os
from functools import wraps
from datetime import datetime
from typing import Type

import arel
from fastapi.responses import HTMLResponse, Response
from fastapi.templating import Jinja2Templates
from jinjax import Catalog

from .age import age


constants = {
    "RELEASE": os.environ["TODOER_RELEASE"],
    "ENV": os.environ["TODOER_ENV"],
}
hot_reload = arel.HotReload(
    paths=[
        arel.Path("app"),
        arel.Path("htm"),
        arel.Path("components"),
        arel.Path("public"),
        arel.Path("styles"),
        arel.Path("tailwind.config.js"),
    ]
)
jinja2 = Jinja2Templates(directory="htm")
jinja2.env.globals["DEBUG"] = os.environ["TODOER_ENV"] == "local"
if os.environ["TODOER_ENV"] == "local":
    jinja2.env.globals["hot_reload"] = hot_reload

jinx = Catalog(jinja_env=jinja2.env)
jinx.add_folder("components")


def filter_age(dateiso: str) -> str:
    return age(datetime.fromisoformat(dateiso))


def htmx(tpl: str, resp: Type[Response] = HTMLResponse):
    def decorator(func):
        @wraps(func)
        async def wrapper(*args, **kwargs):
            ret = await func(*args, **kwargs)
            data = {}
            code = 200
            headers = {}
            if type(ret) is tuple:
                data = ret[0]
                code = ret[1] if len(ret) > 1 else 200
                headers = ret[2] if len(ret) > 2 else {}
            else:
                data = ret
            return resp(jinx.render(tpl, **data), code, headers)
        return wrapper
    return decorator
