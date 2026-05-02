import re


def slugify_title(title: str) -> str:
    s = title.strip()
    s = re.sub(r'^[#@$%]+', '', s).strip()
    s = re.sub(r'\s+', '-', s)
    s = re.sub(r'-+', '-', s)
    return s.strip('-')
