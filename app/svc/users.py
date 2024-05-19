import logging
from uuid import uuid4
from typing import Mapping
import shortuuid

from app.repo import RepoDB
from . import spaces


logger = logging.getLogger(__name__)


async def signup(
    db: RepoDB, email: str, name: str, pwd: str
) -> tuple[Mapping, Mapping, Mapping, Mapping]:
    confirm_code = shortuuid.encode(uuid4())[:7]
    async with db.transaction():
        user = await db.for_anon().users().insert(
            name, email, pwd, confirm_code
        )
        inbox = await spaces.create(
            user, db, spaces.SPACE_INBOX_TITLE, spaces.SPACE_INBOX_DESCR
        )
    # TODO send confirmation email
    logger.info("sending confirmation email should happen here")
    return user, *inbox
