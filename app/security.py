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


_DUMMY_HASH = get_password_hash("dummy-timing-safe-placeholder")


async def authenticate_user(urepo: UserForAnon, email: str, password: str):
    user = await urepo.by_email(email)
    if not user:
        verify_password(password, _DUMMY_HASH)
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


PENDING_2FA_EXPIRE_MINUTES = 5


def create_pending_2fa_token(email: str) -> str:
    """Create a short-lived JWT that proves password was verified."""
    expire = datetime.now(timezone.utc) + timedelta(
        minutes=PENDING_2FA_EXPIRE_MINUTES
    )
    return jwt.encode(
        {"sub": email, "purpose": "2fa_pending", "exp": expire},
        SECRET_KEY,
        algorithm=ALGORITHM,
    )


def verify_pending_2fa_token(token: str) -> str | None:
    """Verify a pending 2FA token. Returns email or None."""
    try:
        payload = jwt.decode(token, SECRET_KEY, algorithms=[ALGORITHM])
        if payload.get("purpose") != "2fa_pending":
            return None
        return payload.get("sub")
    except Exception:
        return None


def set_pending_2fa_cookie(res: Response, email: str):
    token = create_pending_2fa_token(email)
    res.set_cookie(
        key="pending_2fa",
        value=token,
        expires=datetime.now(timezone.utc)
        + timedelta(minutes=PENDING_2FA_EXPIRE_MINUTES),
        secure=not is_dev,
        httponly=True,
        samesite="lax",
    )
    return res


def unset_pending_2fa_cookie(res: Response):
    res.delete_cookie(key="pending_2fa")
    return res
