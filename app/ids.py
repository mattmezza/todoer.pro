import uuid
import shortuuid


def new_ids(max_len: int = 7) -> tuple[str, str]:
    u = uuid.uuid4()
    s = shortuuid.encode(u)[:max_len]
    return str(u), s
