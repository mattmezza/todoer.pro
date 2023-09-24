import os
from datetime import timedelta
from typing import Annotated

from databases import Database
from fastapi import (
    APIRouter,
    Depends,
    Form,
    HTTPException,
    Request,
    status,
)
from fastapi.responses import RedirectResponse
from fastapi.security import OAuth2PasswordBearer, OAuth2PasswordRequestForm
from fastapi.templating import Jinja2Templates
from pydantic import BaseModel

from app.dependencies import get_db, get_tpl
from app.security import (
    ACCESS_TOKEN_EXPIRE_MINUTES,
    authenticate_user,
    create_access_token,
    set_auth_cookie,
    unset_auth_cookie,
)


class Token(BaseModel):
    access_token: str
    token_type: str


is_dev = os.environ['ENV'] == "dev"
oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")
router = APIRouter()


# @router.post("/token", response_model=Token)
async def login_api(
    form_data: Annotated[OAuth2PasswordRequestForm, Depends()],
    db: Database = Depends(get_db),
):
    user = await authenticate_user(db, form_data.username, form_data.password)
    if not user:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Incorrect username or password",
            headers={"WWW-Authenticate": "Bearer"},
        )
    access_token_expires = timedelta(minutes=ACCESS_TOKEN_EXPIRE_MINUTES)
    access_token = create_access_token(
        data={"sub": user.uuid},
        expires_delta=access_token_expires,
    )
    return {"access_token": access_token, "token_type": "bearer"}


@router.post("/auth", response_model=Token)
async def login(
    email: Annotated[str, Form()],
    password: Annotated[str, Form()],
    path: Annotated[str | None, Form()] = None,
    db: Database = Depends(get_db),
):
    user = await authenticate_user(db, email, password)
    if not user:
        return RedirectResponse("/login?e=auth", 303)
    await db.execute(
        "UPDATE users SET last_login_at = CURRENT_TIMESTAMP WHERE uuid = :uuid",
        {"uuid": str(user.uuid)},
    )
    res = RedirectResponse(path or "/", 303)
    set_auth_cookie(res, user)
    return res


@router.get("/auth/logout", response_model=Token)
async def logout():
    res = RedirectResponse("/login", 303)
    unset_auth_cookie(res)
    return res


@router.get("/login")
async def login_view(
    req: Request,
    p: str | None = None,
    e: str | None = None,
    tpl: Jinja2Templates = Depends(get_tpl),
):
    return tpl.TemplateResponse(
        "pages/login.htm",
        {"request": req, "path": p, "error": e},
    )


@router.get("/signup")
async def signup_view(
    req: Request,
    tpl: Jinja2Templates = Depends(get_tpl),
):
    return tpl.TemplateResponse(
        "pages/signup.htm",
        {"request": req},
    )
