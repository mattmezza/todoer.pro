import os
from datetime import datetime

from .age import age


constants = {
    "RELEASE": os.environ["TODOER_RELEASE"],
    "ENV": os.environ["TODOER_ENV"],
}


def filter_age(dateiso: str) -> str:
    return age(datetime.fromisoformat(dateiso))
