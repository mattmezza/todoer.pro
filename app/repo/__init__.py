from typing import Mapping

from databases import Database

from .spaces import SpaceForUser
from .lists import ListForUser
from .sections import SectionForUser


class ForUser:
    def __init__(self, db: Database, user: Mapping):
        self.db = db
        self.user = user

    def spaces(self) -> SpaceForUser:
        return SpaceForUser(self.user, self.db)

    def lists(self) -> ListForUser:
        return ListForUser(self.user, self.db)

    def sections(self) -> SectionForUser:
        return SectionForUser(self.user, self.db)


class RepoDB:
    def __init__(self, db: Database):
        self.db = db

    def for_user(self, user: Mapping) -> ForUser:
        return ForUser(self.db, user)
