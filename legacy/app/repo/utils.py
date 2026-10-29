import uuid
from typing import Mapping

import shortuuid
from databases import Database


def new_ids(max_len: int = 7) -> tuple[str, str]:
    u = uuid.uuid4()
    s = shortuuid.encode(u)[:max_len]
    return str(u), s


class AnonymousRepo:
    def __init__(self, db: Database):
        self.db = db


class LoggedInUserRepo(AnonymousRepo):
    def __init__(self, user: Mapping, db: Database):
        super().__init__(db)
        self.user = user
