import hashlib
import os
import arel
import time
from contextlib import asynccontextmanager
from jinjax import Catalog
from typing import Annotated, Callable, Mapping

from databases import Database
from fastapi import Cookie, Depends, FastAPI, HTTPException, Request, status
from fastapi.responses import RedirectResponse
from fastapi.security import OAuth2PasswordBearer
from fastapi.templating import Jinja2Templates
from jose import JWTError, jwt

from .security import ALGORITHM, SECRET_KEY, get_user
from .templating import filter_age

db = Database(os.environ["TODOER_DB_URL"])
jinja2 = Jinja2Templates(directory="htm")
catalog = Catalog(jinja_env=jinja2.env)
catalog.add_folder("components")
oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")

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
jinja2.env.globals["DEBUG"] = True
jinja2.env.globals["hot_reload"] = hot_reload


@asynccontextmanager
async def lifespan(app: FastAPI):
    await db.connect()
    yield
    await db.disconnect()


def get_tpl():
    jinja2.env.globals["ENV"] = os.environ["TODOER_ENV"]
    jinja2.env.globals["epoch"] = lambda: str(int(time.time()))
    if os.environ["TODOER_ENV"] == "local":
        jinja2.env.globals["DEBUG"] = True
        jinja2.env.globals["hot_reload"] = hot_reload
    jinja2.env.filters["age"] = filter_age
    return jinja2


Renderer = Callable


def renderer() -> Renderer:
    catalog.jinja_env.globals["ENV"] = os.environ["TODOER_ENV"]
    catalog.jinja_env.globals["RELEASE"] = os.environ["TODOER_RELEASE"]
    if os.environ["TODOER_ENV"] == "local":
        catalog.jinja_env.globals["DEBUG"] = True
        catalog.jinja_env.globals["hot_reload"] = hot_reload
    jinja2.env.filters["age"] = filter_age
    catalog.jinja_env.globals["epoch"] = lambda: str(int(time.time()))
    catalog.jinja_env.filters["age"] = filter_age
    return catalog.render


def get_db():
    return db


async def get_current_user_api(
    token: Annotated[str, Depends(oauth2_scheme)],
    db: Annotated[Database, Depends(get_db)],
):
    credentials_exception = HTTPException(
        status_code=status.HTTP_401_UNAUTHORIZED,
        detail="Could not validate credentials",
        headers={"WWW-Authenticate": "Bearer"},
    )
    try:
        payload = jwt.decode(token, SECRET_KEY, algorithms=[ALGORITHM])
        email: str = payload.get("sub")
        if email is None:
            raise credentials_exception
    except JWTError:
        raise credentials_exception
    user = await get_user(db, email)
    if user is None:
        raise credentials_exception
    return user


async def get_current_active_user_api(
    current_user: Annotated[Mapping, Depends(get_current_user_api)]
):
    if current_user.disabled:
        raise HTTPException(status_code=400, detail="Inactive user")
    return current_user


async def get_current_user(
    req: Request,
    db: Annotated[Database, Depends(get_db)],
    auth: str | None = Cookie(None),
) -> Mapping:
    exc = HTTPException(
        status_code=303,
        headers={"Location": f"/login?e=auth&p={req.url.path}"},
    )
    if auth is None:
        raise exc
    try:
        payload = jwt.decode(auth, SECRET_KEY, algorithms=[ALGORITHM])
        email: str = payload.get("sub")
        if email is None:
            raise exc
    except JWTError:
        raise exc
    user = await get_user(db, email)
    if user is None:
        raise exc
    mm = {k: v for (k, v) in user._mapping.items()}
    del mm['password']
    mm['email_hash'] = hashlib.md5(
        str(mm['email']).encode("utf-8")
    ).hexdigest()
    return mm


async def get_current_active_user(
    current_user: Annotated[Mapping, Depends(get_current_user)]
) -> Mapping:
    if current_user["disabled"]:
        return RedirectResponse("/login?e=auth-disabled")
    return current_user
