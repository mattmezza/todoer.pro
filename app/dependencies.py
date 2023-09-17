from contextlib import asynccontextmanager

import databases
from fastapi import FastAPI
from fastapi.templating import Jinja2Templates


DATABASE_URL = "sqlite:///../todo.db"
db = databases.Database(DATABASE_URL)


@asynccontextmanager
async def lifespan(app: FastAPI):
    await db.connect()
    yield
    await db.disconnect()


tpl = Jinja2Templates(directory="tpl")


def get_tpl():
    return tpl


def get_db():
    return db
