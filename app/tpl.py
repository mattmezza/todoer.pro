import io
import os
import re
from functools import wraps
from datetime import datetime
from html.parser import HTMLParser
import time
from typing import Type

import arel
import markdown
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
RE_URL = re.compile(r'(?P<href>(?P<origin>(?:(?P<protocol>[a-zA-Z][a-zA-Z0-9\+]{1,31}):\/\/)?(?:(?P<username>[^\:\s]+)\:(?P<password>[^@\s]+)@)?(?P<hostname>(?:[-a-zA-Z0-9:%._\+~#=]{2,256}\.)*(?P<domain>[-a-zA-Z0-9:%._\+~#=]{2,256}\.(?P<tld>[a-z]{2,6}))|localhost)(?:\:(?P<port>[1-9][0-9]{,4}))?)(?P<pathname>\/[^\?\s#]*)*(?P<search>\?(?:[^=\s]+=[^&#\s]*)(?:(?:[^=\s]+=[^&#\s]*)*))?(?P<hash>#[^\b\s\#]*)?)')

def linkify_urls(text: str) -> str:
    # Linkify URLs
    # uncomment the following two lines to debug the regex
    # matches = [m.groupdict() for m in RE_URL.finditer(text)]
    # print(matches)
    text = re.sub(
        RE_URL, "<a href='\\1'>\\g<origin></a>", text
    )
    return Markup(text)


def linkify_mentions(text: str) -> str:
    # Linkify email addresses
    text = re.sub(
        r'([a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,})',
        "<a href='mailto:\\1'>\\1</a>",
        text,
        flags=re.MULTILINE,
    )
    # Linkify @spaces
    text = re.sub(
        r'@(?P<name>[a-z][a-z0-9-]{,62}[a-z0-9])\b',
        "<a href='/space/\\g<name>'>@\\g<name></a>",
        text,
        flags=re.MULTILINE,
    )
    # Linkify #lists
    text = re.sub(
        r'#(?P<name>[a-z][a-z0-9-]{,62}[a-z0-9])\b',
        "<a href='/list/\\g<name>'>#\\g<name></a>",
        text,
        flags=re.MULTILINE,
    )
    # Linkify %tasks
    text = re.sub(
        r'%(?P<id>[a-zA-Z0-9]{7})',
        "<a href='/task/\\g<id>'>%\\g<id></a>",
        text,
        flags=re.MULTILINE,
    )
    return Markup(text)


def linkify(text: str) -> str:
    return linkify_mentions(linkify_urls(text))


mdown = markdown.Markdown(
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


def md(text: str) -> Markup:
    return Markup(mdown.convert(text))


def strip_md(text: str) -> str:
    return strip_html(md(text))


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
jinja2.env.filters['markdown'] = md
jinja2.env.filters['strip_markdown'] = strip_md
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
            return resp(jinx.render(tpl, **data), code, headers)

        return wrapper

    return decorator
