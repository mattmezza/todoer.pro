from collections import defaultdict
from datetime import date, timedelta

from typing import Annotated, Mapping

from fastapi import APIRouter, Depends, Query, Request

from app import tpl
from app.deps import RepoDB, get_current_active_user, repo_db

router = APIRouter()

WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
MONTHS = [
    "", "Jan", "Feb", "Mar", "Apr", "May", "Jun",
    "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
]
MONTHS_FULL = [
    "", "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
]


def _month_bounds(dt: date) -> tuple[date, date]:
    """Return (first_day, last_day) of the month containing dt."""
    first = dt.replace(day=1)
    if dt.month == 12:
        last = dt.replace(year=dt.year + 1, month=1, day=1) - timedelta(days=1)
    else:
        last = dt.replace(month=dt.month + 1, day=1) - timedelta(days=1)
    return first, last


def _iso_week_bounds(dt: date, full: bool = False) -> tuple[date, date]:
    """Return (monday, sunday) of the ISO week containing dt.

    If full=False (working week), return (monday, friday).
    """
    monday = dt - timedelta(days=dt.weekday())
    if full:
        return monday, monday + timedelta(days=6)
    else:
        return monday, monday + timedelta(days=4)


def _weekend_bounds(dt: date) -> tuple[date, date]:
    """Return (saturday, sunday) of the week containing dt."""
    saturday = dt - timedelta(days=dt.weekday()) + timedelta(days=5)
    return saturday, saturday + timedelta(days=1)


def _build_calendar_grid(first: date, last: date) -> list[list[date | None]]:
    """Build a 6-week grid (list of weeks, each week is 7 days).

    None = day outside the month.
    """
    # Start from Monday of the week containing first
    start = first - timedelta(days=first.weekday())
    # End on Sunday of the week containing last
    end = last + timedelta(days=6 - last.weekday())

    weeks = []
    current = start
    while current <= end:
        week = []
        for _ in range(7):
            if current.month == first.month:
                week.append(current)
            else:
                week.append(None)
            current += timedelta(days=1)
        weeks.append(week)
    return weeks


def _fmt_date(dt: date) -> str:
    return dt.isoformat()


@router.get("/date")
@tpl.htmx("Page.Date")
async def date_view(
    req: Request,
    view: Annotated[str, Query()] = "month",
    date_str: Annotated[str | None, Query(alias="date")] = None,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    # Parse focus date
    today = date.today()
    try:
        focus = date.fromisoformat(date_str) if date_str else today
    except (ValueError, TypeError):
        focus = today

    urepo = db.for_user(user)

    # Determine date range based on view
    if view == "day":
        start = end = focus
        tasks = await urepo.tasks().by_date_range(_fmt_date(start), _fmt_date(end))
        title = focus.strftime("%A, %B %d, %Y")
        prev_date = _fmt_date(focus - timedelta(days=1))
        next_date = _fmt_date(focus + timedelta(days=1))
        prev_label = "Yesterday"
        next_label = "Tomorrow"
    elif view == "week":
        start, end = _iso_week_bounds(focus, full=True)
        tasks = await urepo.tasks().by_date_range(_fmt_date(start), _fmt_date(end))
        title = f"{MONTHS[start.month]} {start.day} – {MONTHS[end.month]} {end.day}, {end.year}"
        prev_date = _fmt_date(start - timedelta(days=7))
        next_date = _fmt_date(start + timedelta(days=7))
        prev_label = "Prev week"
        next_label = "Next week"
    elif view == "work-week":
        start, end = _iso_week_bounds(focus, full=False)
        tasks = await urepo.tasks().by_date_range(_fmt_date(start), _fmt_date(end))
        title = f"{MONTHS[start.month]} {start.day} – {MONTHS[end.month]} {end.day}, {end.year}"
        prev_date = _fmt_date(start - timedelta(days=7))
        next_date = _fmt_date(start + timedelta(days=7))
        prev_label = "Prev week"
        next_label = "Next week"
    elif view == "weekend":
        start, end = _weekend_bounds(focus)
        tasks = await urepo.tasks().by_date_range(_fmt_date(start), _fmt_date(end))
        title = f"{MONTHS[start.month]} {start.day} – {MONTHS[end.month]} {end.day}, {end.year}"
        prev_date = _fmt_date(start - timedelta(days=7))
        next_date = _fmt_date(start + timedelta(days=7))
        prev_label = "Prev weekend"
        next_label = "Next weekend"
    else:  # month
        start, end = _month_bounds(focus)
        tasks = await urepo.tasks().by_date_range(_fmt_date(start), _fmt_date(end))
        title = f"{MONTHS_FULL[focus.month]} {focus.year}"
        prev_date = _fmt_date(start - timedelta(days=1))
        next_date = _fmt_date(end + timedelta(days=1))
        prev_label = MONTHS_FULL[(focus.month - 2) % 12 + 1] if focus.month > 1 else "Prev"
        next_label = MONTHS_FULL[focus.month % 12 + 1] if focus.month < 12 else "Next"

    # Group tasks by due_date
    by_date = defaultdict(list)
    for t in tasks:
        by_date[t["due_date"]].append(t)

    # Calendar grid for month view
    calendar_grid = None
    months = []
    if view == "month":
        calendar_grid = _build_calendar_grid(start, end)
        # Build month navigation
        for offset in range(-2, 3):
            m = focus.month + offset
            y = focus.year
            while m < 1:
                m += 12
                y -= 1
            while m > 12:
                m -= 12
                y += 1
            months.append({"year": y, "month": m, "label": MONTHS_FULL[m][:3]})

    # Week column headers
    weekdays = []
    if view in ("week", "work-week", "weekend"):
        for i in range(7):
            if view == "work-week" and i >= 5:
                continue
            if view == "weekend" and i < 5:
                continue
            d = start + timedelta(days=i)
            weekdays.append({"date": d, "label": WEEKDAYS[d.weekday()], "day": d.day, "iso": _fmt_date(d)})

    # Day view — also provide hours breakdown
    hours = list(range(24))

    return {
        "spaces": await urepo.spaces().all(),
        "user": user,
        "view": view,
        "focus_date": _fmt_date(focus),
        "today": _fmt_date(today),
        "title": title,
        "start_date": _fmt_date(start),
        "end_date": _fmt_date(end),
        "prev_date": prev_date,
        "next_date": next_date,
        "prev_label": prev_label,
        "next_label": next_label,
        "tasks": tasks,
        "by_date": dict(by_date),
        "calendar_grid": calendar_grid,
        "months": months,
        "weekdays": weekdays,
        "hours": hours,
        "total_tasks": len(tasks),
    }
