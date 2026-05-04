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


def _build_calendar_grid(first: date, last: date) -> list[dict]:
    """Build a 6-week grid.

    Returns list of dicts:
        {"week_num": int, "monday_iso": str, "days": list[date | None]}
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
        week_num = start.isocalendar()[1]
        weeks.append({
            "week_num": week_num,
            "monday_iso": _fmt_date(start),
            "days": week,
        })
        start += timedelta(days=7)
    return weeks


def _fmt_date(dt: date) -> str:
    return dt.isoformat()


@router.get("/calendar")
@tpl.htmx("Page.Calendar")
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
    elif view == "year":
        start = focus.replace(month=1, day=1)
        end = focus.replace(month=12, day=31)
        tasks = await urepo.tasks().by_date_range(_fmt_date(start), _fmt_date(end))
        title = str(focus.year)
        prev_date = _fmt_date(focus.replace(year=focus.year - 1, month=1, day=1))
        next_date = _fmt_date(focus.replace(year=focus.year + 1, month=1, day=1))
        prev_label = str(focus.year - 1)
        next_label = str(focus.year + 1)
        # Build per-month day sets for the mini-calendar indicators
        month_days = []
        for m in range(1, 13):
            m_first = start.replace(month=m)
            if m == 12:
                m_last = start.replace(year=start.year + 1, month=1, day=1) - timedelta(days=1)
            else:
                m_last = start.replace(month=m + 1, day=1) - timedelta(days=1)
            days_with_tasks = set()
            for t in tasks:
                td = t["due_date"]
                if td:
                    d = date.fromisoformat(td)
                    if d.month == m:
                        days_with_tasks.add(d.day)
            month_days.append({
                "month": m,
                "label": MONTHS_FULL[m][:3],
                "label_full": MONTHS_FULL[m],
                "first": m_first,
                "last": m_last,
                "days_with_tasks": sorted(days_with_tasks),
                "total_tasks": sum(1 for t in tasks if t["due_date"] and date.fromisoformat(t["due_date"]).month == m),
            })
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
    prev_year = focus.year - 1
    next_year = focus.year + 1
    if view == "month":
        calendar_grid = _build_calendar_grid(start, end)
        # Build all 12 months for the current year
        for m in range(1, 13):
            months.append({
                "year": focus.year,
                "month": m,
                "label": MONTHS_FULL[m][:3],
            })

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
        "prev_year": prev_year,
        "next_year": next_year,
        "weekdays": weekdays,
        "month_days": month_days if view == "year" else None,
        "hours": hours,
        "total_tasks": len(tasks),
    }
