from typing import Generic, TypeVar
from fastapi import HTTPException


T = TypeVar("T")
Other = TypeVar("Other")


class Opt(Generic[T]):
    def __init__(self, val: T | None):
        self.val = val

    def or_none(self) -> T | None:
        return self.val

    def or_other(self, other: Other) -> T | Other:
        if self.val is None:
            return other
        return self.val

    def or_404(self, msg: str = "Item not found") -> T:
        return self.or_exception(HTTPException(status_code=404, detail=msg))

    def or_400(self, msg: str = "Bad request") -> T:
        return self.or_exception(HTTPException(status_code=400, detail=msg))

    def or_exception(self, e: HTTPException) -> T:
        if self.val is None:
            raise e
        return self.val
