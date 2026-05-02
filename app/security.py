import os
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from typing import Mapping

from fastapi.responses import Response
import bcrypt
from jose import jwt

from app.repo import UserForAnon

SECRET_KEY = os.environ["TODOER_SECRET_KEY"]
ALGORITHM = "HS256"
ACCESS_TOKEN_EXPIRE_MINUTES = int(
    os.environ["TODOER_ACCESS_TOKEN_EXPIRE_MINUTES"]
)
AUTH_COOKIE_EXPIRE_MINUTES = int(
    os.environ["TODOER_AUTH_COOKIE_EXPIRE_MINUTES"]
)

is_dev = os.environ['TODOER_ENV'] == "local"


@dataclass(frozen=True, kw_only=True)
class Token:
    access_token: str
    token_type: str


def verify_password(plain_password: str, hashed_password: str) -> bool:
    return bcrypt.checkpw(
        plain_password.encode(), hashed_password.encode()
    )


def get_password_hash(password: str) -> str:
    return bcrypt.hashpw(
        password.encode(), bcrypt.gensalt()
    ).decode()


# Pre-computed bcrypt hash used as a dummy target when the user doesn't
# exist, so we always spend time on a bcrypt verify and avoid leaking
# whether an email is registered via response-time differences.
_DUMMY_HASH = pwd_context.hash("dummy-timing-safe-placeholder")


async def authenticate_user(urepo: UserForAnon, email: str, password: str):
    user = await urepo.by_email(email)
    if not user:
        # Perform a dummy verify to keep constant timing
        pwd_context.verify(password, _DUMMY_HASH)
        return False
    if not verify_password(password, user["password"]):
        return False
    return user


def create_access_token(data: dict, expires_delta: timedelta | None = None):
    to_encode = data.copy()
    if expires_delta:
        expire = datetime.now(timezone.utc) + expires_delta
    else:
        expire = datetime.now(timezone.utc) + timedelta(
            minutes=ACCESS_TOKEN_EXPIRE_MINUTES
        )
    to_encode.update({"exp": expire})
    return jwt.encode(to_encode, SECRET_KEY, algorithm=ALGORITHM)


def set_auth_cookie(res: Response, user: Mapping):
    access_token_expires = timedelta(minutes=ACCESS_TOKEN_EXPIRE_MINUTES)
    access_token = create_access_token(
        data={"sub": user["email"]},
        expires_delta=access_token_expires,
    )
    res.set_cookie(
        key="auth",
        value=access_token,
        expires=datetime.now(timezone.utc)
        + timedelta(minutes=AUTH_COOKIE_EXPIRE_MINUTES),
        secure=not is_dev,
        httponly=True,
        samesite="lax",  # can be one of ["lax", "strict", "none"]]
    )
    return res


def unset_auth_cookie(res):
    res.delete_cookie(key="auth")
    return res
