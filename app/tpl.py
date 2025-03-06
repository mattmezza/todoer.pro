import io
import os
import re
from functools import partial, wraps
from datetime import datetime
from html.parser import HTMLParser
import time
from typing import Type

import arel
import markdown as md
from markupsafe import Markup
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


class HTMLStripper(HTMLParser):
    def __init__(self):
        super().__init__()
        self.reset()
        self.strict = False
        self.convert_charrefs = True
        self.text = io.StringIO()

    def handle_data(self, d):
        self.text.write(d)

    def get_data(self):
        return self.text.getvalue()


def strip_html(html):
    s = HTMLStripper()
    s.feed(html)
    return s.get_data()


RE_EMAIL = re.compile(
    r'(^|(?<=\s))(?P<email>[a-zA-Z0-9._+-]+@[a-zA-Z0-9.-]+(\.[a-zA-Z]{2,6})?)',
    re.MULTILINE,
)
SUB_EMAIL = "<a href='mailto:\\g<email>'>\\g<email></a>"
linkify_emails = partial(re.sub, RE_EMAIL, SUB_EMAIL)

# RE_SPACE = re.compile(
#     r'(^|(?<=\s))@(?P<name>[a-z][a-z0-9-]{,62}[a-z0-9])', flags=re.MULTILINE
# )
RE_SPACE = re.compile(
    r'(^|(?<=\s))@(?P<id>[a-zA-Z0-9]{7})(?![a-zA-Z0-9])', flags=re.MULTILINE
)
# SUB_SPACE = "<a href='/space/\\g<name>'>@\\g<name></a>"
SUB_SPACE = "<a href='/space/\\g<id>'>@\\g<id></a>"
linkify_spaces = partial(re.sub, RE_SPACE, SUB_SPACE)

# RE_LIST = re.compile(
#     r'(^|(?<=\s))#(?P<name>[a-z][a-z0-9-]{,62}[a-z0-9])', flags=re.MULTILINE
# )
RE_LIST = re.compile(
    r'(^|(?<=\s))#(?P<id>[a-zA-Z0-9]{7})(?![a-zA-Z0-9])', flags=re.MULTILINE
)
# SUB_LIST = "<a href='/list/\\g<name>'>#\\g<name></a>"
SUB_LIST = "<a href='/list/\\g<id>'>#\\g<id></a>"
linkify_lists = partial(re.sub, RE_LIST, SUB_LIST)

RE_TASK = re.compile(
    r'(^|(?<=\s))%(?P<id>[a-zA-Z0-9]{7})(?![a-zA-Z0-9])', flags=re.MULTILINE
)
SUB_TASK = "<a href='/task/\\g<id>'>%\\g<id></a>"
linkify_tasks = partial(re.sub, RE_TASK, SUB_TASK)


"""
The following regular expression finds URLs in text. It extracts the following:
- href: the full URL
- origin: the full URL without the query string or hash
- protocol: the URL protocol (http, https, etc.)
- username: the URL username
- password: the URL password
- hostname: the URL hostname
- domain: the URL domain
- tld: the URL top-level domain
- port: the URL port
- pathname: the URL pathname
- search: the URL search string
- hash: the URL hash string
"""
RE_URL = re.compile(
    r'(^|(?<=\s))(?P<href>(?P<origin>(?:(?P<protocol>[a-zA-Z][a-zA-Z0-9\+]{1,31}):\/\/)?(?:(?P<username>[\w\-]+)\:(?P<password>[^@\s]+)@)?(?P<hostname>(?:[-a-zA-Z0-9:%._\+~#=]{2,256}\.)*(?P<domain>[-a-zA-Z0-9:%._\+~#=]{2,256}\.(?P<tld>[a-z]{2,6}))|localhost)(?:\:(?P<port>[1-9][0-9]{,4}))?)(?P<pathname>\/[^\?\s#]*)*(?P<search>\?(?:[^=\s]+=[^&#\s]*)(?:(?:[^=\s]+=[^&#\s]*)*))?(?P<hash>#[^\b\s\#]*)?)',
    flags=re.MULTILINE,
)


def linkify_urls(text: str) -> str:
    text = re.sub(RE_URL, "<a href='\\g<href>'>\\g<origin></a>", text)
    text = re.sub(
        r'href=[\"\'](?P<url>[^\/\?][^\"\':@]*)[\"\']',
        'href="//\\g<url>"',
        text,
        flags=re.MULTILINE,
    )
    return text


def linkify(text: str, **kwargs) -> str:
    full = kwargs.get("all", False)

    def unset_true(k):
        if full or k not in kwargs:
            return True
        return kwargs.get(k, True)

    if unset_true("emails"):
        text = linkify_emails(text)
    if unset_true("urls"):
        text = linkify_urls(text)
    if unset_true("spaces"):
        text = linkify_spaces(text)
    if unset_true("lists"):
        text = linkify_lists(text)
    if unset_true("tasks"):
        text = linkify_tasks(text)
    return Markup(text) if kwargs.get("safe", True) else text


linkify_mentions = partial(linkify, emails=False, urls=False, safe=False)


mdown = md.Markdown(
    safe_mode=True,
    extensions=[
        'pymdownx.magiclink',
        'pymdownx.emoji',
        'pymdownx.tilde',
        'meta',
    ],
    extension_configs={
        "pymdownx.magiclink": {
            "hide_protocol": True,
        },
    },
)


def markdown(text: str, **kwargs) -> Markup:
    text = linkify(text, **kwargs, emails=False, urls=False)
    text = mdown.convert(text)
    return Markup(text) if kwargs.get("safe", True) else text


def strip_markdown(text: str) -> str:
    text = markdown(text, safe=False)
    text = strip_html(text)
    return text


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
hot_reload_paths = [
    arel.Path("app"),
    arel.Path("components"),
    arel.Path("public"),
]
if os.environ["TODOER_ENV"] == "local":
    hot_reload_paths + [arel.Path("styles"), arel.Path("tailwind.config.js")]
hot_reload = arel.HotReload(paths=hot_reload_paths)
jinja2 = Jinja2Templates(directory="htm")
jinja2.env.filters['markdown'] = markdown
jinja2.env.filters['strip_markdown'] = strip_markdown
jinja2.env.filters['linkify'] = linkify
jinja2.env.filters['linkify_urls'] = linkify_urls
jinja2.env.filters['linkify_mentions'] = linkify_mentions
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
            return resp(
                jinx.render(tpl, **data) if code != 204 else data,
                code,
                headers,
            )

        return wrapper

    return decorator
