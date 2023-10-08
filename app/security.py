import os
from datetime import datetime, timedelta, timezone

from dataclasses import dataclass
from jose import jwt
from passlib.context import CryptContext
from databases import Database

# to get a string like this run:
# openssl rand -hex 32
SECRET_KEY = os.environ["TODOER_SECRET_KEY"]
ALGORITHM = "HS256"
ACCESS_TOKEN_EXPIRE_MINUTES = int(
    os.environ["TODOER_ACCESS_TOKEN_EXPIRE_MINUTES"]
)
AUTH_COOKIE_EXPIRE_MINUTES = int(
    os.environ["TODOER_AUTH_COOKIE_EXPIRE_MINUTES"]
)

pwd_context = CryptContext(schemes=["bcrypt"], deprecated="auto")
is_dev = os.environ['TODOER_ENV'] == "local"


@dataclass(frozen=True, kw_only=True)
class Token:
    access_token: str
    token_type: str


def verify_password(plain_password, hashed_password):
    return pwd_context.verify(plain_password, hashed_password)


def get_password_hash(password):
    return pwd_context.hash(password)


async def get_user(db: Database, email: str):
    return await db.fetch_one(
        "SELECT * FROM users WHERE email = :email",
        {"email": email},
    )


async def authenticate_user(db: Database, email: str, password: str):
    user = await get_user(db, email)
    if not user:
        return False
    if not verify_password(password, user.password):
        return False
    return user


def create_access_token(data: dict, expires_delta: timedelta | None = None):
    to_encode = data.copy()
    if expires_delta:
        expire = datetime.utcnow() + expires_delta
    else:
        expire = datetime.utcnow() + timedelta(
            minutes=ACCESS_TOKEN_EXPIRE_MINUTES
        )
    to_encode.update({"exp": expire})
    encoded_jwt = jwt.encode(to_encode, SECRET_KEY, algorithm=ALGORITHM)
    return encoded_jwt


def set_auth_cookie(res, user):
    access_token_expires = timedelta(minutes=ACCESS_TOKEN_EXPIRE_MINUTES)
    access_token = create_access_token(
        data={"sub": user.email},
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


def unset_auth_cookie(res):
    res.delete_cookie(key="auth")
