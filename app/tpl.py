import os
from functools import wraps
from datetime import datetime
import time
from typing import Type

import arel
from fastapi.responses import HTMLResponse, Response
from fastapi.templating import Jinja2Templates
from jinja2 import pass_context
from jinjax import Catalog


def age(since: datetime) -> str:
    now = datetime.utcnow()
    delta = abs(now - since)
    pa = {}
    pa["day"], pa["second"] = delta.days, delta.seconds

    pa["year"], pa["day"] = pa["day"] // 365, pa["day"] % 365
    pa["month"], pa["day"] = pa["day"] // 30, pa["day"] % 30
    pa["hour"], pa["second"] = pa["second"] // 3600, pa["second"] % 3600
    pa["minute"], pa["second"] = pa["second"] // 60, pa["second"] % 60

    for period in ["year", "month", "day", "hour", "minute", "second"]:
        amount = pa[period]
        if amount > 0:
            if since > now:
                return _fmt_future(amount, period)
            return _fmt_past(amount, period)
    return "now"


def _fmt_past(n: int, s: str):
    if n == 1:
        return "a %s ago" % s
    return "%d %ss ago" % (n, s)


def _fmt_future(n: int, s: str):
    if n == 1:
        return "in a %s" % s
    return "in %d %ss" % (n, s)


def filter_age(dateiso: str) -> str:
    return age(datetime.fromisoformat(dateiso))


@pass_context
def url_for(ctx: dict, name: str, **path_params) -> str:
    req = ctx["req"]
    return req.url_for(name, **path_params)


@pass_context
def cur_path_starts_with(ctx: dict, path: str) -> bool:
    req = ctx["req"]
    return req.url.path.startswith(path)


constants = {
    "RELEASE": os.environ["TODOER_RELEASE"],
    "ENV": os.environ["TODOER_ENV"],
}
hot_reload = arel.HotReload(
    paths=[
        arel.Path("app"),
        arel.Path("components"),
        arel.Path("public"),
        arel.Path("styles"),
        arel.Path("tailwind.config.js"),
    ]
)
jinja2 = Jinja2Templates(directory="htm")

jinja2.env.filters["age"] = filter_age
jinja2.env.globals["epoch"] = lambda: str(int(time.time()))

jinja2.env.globals["DEBUG"] = os.environ["TODOER_ENV"] == "local"
if os.environ["TODOER_ENV"] == "local":
    jinja2.env.globals["hot_reload"] = hot_reload

jinx = Catalog(jinja_env=jinja2.env)
jinx.add_folder("components")
jinx.jinja_env.globals["ENV"] = os.environ["TODOER_ENV"]
jinx.jinja_env.globals["RELEASE"] = os.environ["TODOER_RELEASE"]
jinx.jinja_env.globals["url_for"] = url_for
jinx.jinja_env.globals["cur_path_starts_with"] = cur_path_starts_with
jinx.jinja_env.globals["request"] = None


def htmx(tpl: str, resp: Type[Response] = HTMLResponse):
    def decorator(func):
        @wraps(func)
        async def wrapper(*args, **kwargs):
            req = kwargs.get("req", None)
            assert (
                req
            ), "Don't forget to export 'req: Request' from your FastAPI route."
            jinx.jinja_env.globals["req"] = req
            data = {}
            code = 200
            headers = {}
            ret = await func(*args, **kwargs)
            if type(ret) is tuple:
                data = ret[0]
                code = ret[1] if len(ret) > 1 else 200
                headers = ret[2] if len(ret) > 2 else {}
            else:
                data = ret
            return resp(jinx.render(tpl, **data), code, headers)

        return wrapper

    return decorator
