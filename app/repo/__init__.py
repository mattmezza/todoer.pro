from typing import Mapping

from databases import Database

from .ai_usage import AIUsageForUser
from .attachments import AttachmentForUser
from .bookmarks import BookmarkForUser
from .comments import CommentForUser
from .lists import ListForUser
from .notification_reads import NotificationReadForUser
from .sections import SectionForUser
from .settings import SettingsForUser
from .spaces import SpaceForUser
from .subtasks import SubtaskForUser
from .tasks import TaskForUser
from .users import UserForAnon, UserForUser


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

    def tasks(self) -> TaskForUser:
        return TaskForUser(self.user, self.db)

    def comments(self) -> CommentForUser:
        return CommentForUser(self.user, self.db)

    def subtasks(self) -> SubtaskForUser:
        return SubtaskForUser(self.user, self.db)

    def attachments(self) -> AttachmentForUser:
        return AttachmentForUser(self.user, self.db)

    def bookmarks(self) -> BookmarkForUser:
        return BookmarkForUser(self.user, self.db)

    def ai_usage(self) -> AIUsageForUser:
        return AIUsageForUser(self.user, self.db)

    def notification_reads(self) -> NotificationReadForUser:
        return NotificationReadForUser(self.user, self.db)

    def settings(self) -> SettingsForUser:
        return SettingsForUser(self.user, self.db)

    def users(self) -> UserForUser:
        return UserForUser(self.user, self.db)


class ForAnon:
    def __init__(self, db: Database):
        self.db = db

    def users(self) -> UserForAnon:
        return UserForAnon(self.db)


class RepoDB:
    def __init__(self, db: Database):
        self.db = db

    def for_user(self, user: Mapping) -> ForUser:
        return ForUser(self.db, user)

    def for_anon(self) -> ForAnon:
        return ForAnon(self.db)

    def transaction(self):
        return self.db.transaction()
