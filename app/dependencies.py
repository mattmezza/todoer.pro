import hashlib
import os
import time
from contextlib import asynccontextmanager
from typing import Annotated, Callable, Mapping

from databases import Database
from fastapi import Cookie, Depends, FastAPI, HTTPException, Request, status
from fastapi.responses import RedirectResponse
from fastapi.security import OAuth2PasswordBearer
from jose import JWTError, jwt

from .repo import RepoDB
from .security import ALGORITHM, SECRET_KEY
from .templating import hot_reload, jinx, jinja2, filter_age

db = Database(os.environ["TODOER_DB_URL"])
oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")
Renderer = Callable


@asynccontextmanager
async def lifespan(app: FastAPI):
    await db.connect()
    yield
    await db.disconnect()


def renderer() -> Renderer:
    jinx.jinja_env.globals["ENV"] = os.environ["TODOER_ENV"]
    jinx.jinja_env.globals["RELEASE"] = os.environ["TODOER_RELEASE"]
    if os.environ["TODOER_ENV"] == "local":
        jinx.jinja_env.globals["DEBUG"] = True
        jinx.jinja_env.globals["hot_reload"] = hot_reload
    jinja2.env.filters["age"] = filter_age
    jinx.jinja_env.globals["epoch"] = lambda: str(int(time.time()))
    jinx.jinja_env.filters["age"] = filter_age
    return jinx.render


def get_db():
    return db


def repo_db():
    return RepoDB(db)


async def get_current_user_api(
    token: Annotated[str, Depends(oauth2_scheme)],
    db: RepoDB = Depends(repo_db),
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
    user = await db.for_anon().users().by_email(email)
    if user is None:
        raise credentials_exception
    return user


async def get_current_active_user_api(
    current_user: Annotated[Mapping, Depends(get_current_user_api)]
):
    if current_user["disabled"]:
        raise HTTPException(status_code=400, detail="Inactive user")
    return current_user


async def get_current_user(
    req: Request,
    db: RepoDB = Depends(repo_db),
    auth: str | None = Cookie(None),
) -> Mapping:
    exc = HTTPException(
        status_code=303,
        headers={
            "Location": f"/signin?err=auth-needs-signin&goto={req.url.path}"
        },
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
    user = await db.for_anon().users().by_email(email)
    if user is None:
        raise exc
    mm = {k: v for (k, v) in user.items()}
    del mm['password']
    mm['email_hash'] = hashlib.md5(
        str(mm['email']).encode("utf-8")
    ).hexdigest()
    return mm


async def get_current_active_user(
    current_user: Annotated[Mapping, Depends(get_current_user)]
) -> Mapping:
    if current_user["disabled"]:
        return RedirectResponse("/signin?err=auth-disabled")
    return current_user
