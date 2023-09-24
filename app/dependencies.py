import os
from contextlib import asynccontextmanager
import hashlib

from typing import Annotated, Mapping
from databases import Database
from jose import JWTError, jwt
from fastapi import Depends, FastAPI, HTTPException, Cookie, status, Request
from fastapi.responses import RedirectResponse
from fastapi.security import OAuth2PasswordBearer
from fastapi.templating import Jinja2Templates

from .security import ALGORITHM, SECRET_KEY, get_user


db = Database(os.environ["DB_URL"])
tpl = Jinja2Templates(directory="htm")
oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")


@asynccontextmanager
async def lifespan(app: FastAPI):
    await db.connect()
    yield
    await db.disconnect()


def get_tpl():
    return tpl


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
    mm['email_hash'] = hashlib.md5(str(mm['email']).encode("utf-8")).hexdigest()
    return mm


async def get_current_active_user(
    current_user: Annotated[Mapping, Depends(get_current_user)]
) -> Mapping:
    if current_user["disabled"]:
        return RedirectResponse("/login?e=auth-disabled")
    return current_user
