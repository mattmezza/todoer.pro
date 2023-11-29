import os
from datetime import timedelta

STORAGE_ACCESS_KEY_ID = os.environ['STORAGE_ACCESS_KEY_ID']
STORAGE_SECRET_KEY = os.environ['STORAGE_SECRET_KEY']
STORAGE_DEFAULT_REGION = os.environ['STORAGE_DEFAULT_REGION']
STORAGE_ENDPOINT = os.environ['STORAGE_ENDPOINT']
STORAGE_HOST = os.environ['STORAGE_HOST']

"""After this amount of time, signed url will expire."""
URL_SIGNED_VALIDITY_DURATION_DEFAULT = timedelta(minutes=30)


def put(key: str, content: bytes) -> str:
    # TODO - add implementation
    return ""


def delete(key: str):
    # TODO - add implementation
    return None


def url_signed_for(file_id: str, duration: timedelta) -> str:
    # TODO - add implementation
    return ""
