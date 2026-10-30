const weekdays = [
  'sunday',
  'monday',
  'tuesday',
  'wednesday',
  'thursday',
  'friday',
  'saturday',
];
const months = [
  'january',
  'february',
  'march',
  'april',
  'may',
  'june',
  'july',
  'august',
  'september',
  'october',
  'november',
  'december',
];
const stamp = (d) =>
  `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
const calendarDate = (y, m, d) => {
  const value = new Date(y, m, d, 12);
  return value.getFullYear() === y && value.getMonth() === m && value.getDate() === d
    ? value
    : null;
};
// Calendar arithmetic deliberately avoids elapsed milliseconds across DST changes.
export function shortcutDate(token, now = new Date()) {
  token = token.toLowerCase();
  const date = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 12);
  if (['today', 'td', 't'].includes(token)) return stamp(date);
  if (['tomorrow', 'tomo', 'tmr', 'tmrw'].includes(token)) {
    date.setDate(date.getDate() + 1);
    return stamp(date);
  }
  if (['nextweek', 'nw'].includes(token)) {
    date.setDate(date.getDate() + ((8 - date.getDay()) % 7 || 7));
    return stamp(date);
  }
  if (['nextmonth', 'nm'].includes(token))
    return stamp(new Date(date.getFullYear(), date.getMonth() + 1, 1, 12));
  if (['weekend', 'nextweekend'].includes(token)) {
    date.setDate(date.getDate() + ((6 - date.getDay() + 7) % 7 || 7));
    return stamp(date);
  }
  const weekday = weekdays.findIndex(
    (day) =>
      token === day ||
      token === day.slice(0, 3) ||
      token === `next${day}` ||
      token === `next${day.slice(0, 3)}`,
  );
  if (weekday >= 0) {
    const offset = token.startsWith('next')
      ? ((8 - date.getDay()) % 7 || 7) + ((weekday + 6) % 7)
      : (weekday - date.getDay() + 7) % 7 || 7;
    date.setDate(date.getDate() + offset);
    return stamp(date);
  }
  const relative = /^in(\d+)(d|days?|w|weeks?|m|months?|y|years?)$/.exec(token);
  if (relative) {
    const amount = Number(relative[1]),
      unit = relative[2][0];
    // Bound capture dates to ten years, matching the existing relative-day limit.
    if (
      !Number.isSafeInteger(amount) ||
      amount < 1 ||
      amount > { d: 3650, w: 520, m: 120, y: 10 }[unit]
    )
      return null;
    if (unit === 'd' || unit === 'w')
      date.setDate(date.getDate() + amount * (unit === 'w' ? 7 : 1));
    else {
      const day = date.getDate();
      date.setDate(1);
      date.setMonth(date.getMonth() + amount * (unit === 'y' ? 12 : 1));
      date.setDate(
        Math.min(day, new Date(date.getFullYear(), date.getMonth() + 1, 0).getDate()),
      );
    }
    return stamp(date);
  }
  const iso = /^(\d{4})-(\d{2})-(\d{2})$/.exec(token);
  if (iso) {
    const value = calendarDate(Number(iso[1]), Number(iso[2]) - 1, Number(iso[3]));
    return value ? stamp(value) : null;
  }
  const named =
    /^([a-z]+)(\d{1,2})$/.exec(token) ||
    /^(\d{1,2})([a-z]+)$/
      .exec(token)
      ?.slice(0)
      .map((v, i, a) => (i === 1 ? a[2] : i === 2 ? a[1] : v));
  if (named) {
    const month = months.findIndex((m) => named[1] === m || named[1] === m.slice(0, 3));
    if (month < 0) return null;
    for (let year = date.getFullYear(); year <= date.getFullYear() + 8; year++) {
      const value = calendarDate(year, month, Number(named[2]));
      if (value && value >= date) return stamp(value);
    }
  }
  return null;
}
export function captureDateSpans(text, { now, naturalDates = true, tokens = [] }) {
  const groups = { planned: [], due: [] },
    warnings = [],
    spans = [];
  const outside = (start, end) => !tokens.some((t) => start < t.end && end > t.start);
  for (const match of text.matchAll(
    /(?:^|\s)([.-])([a-z0-9][a-z0-9-]*)(?=\s|$|[,.!?;:])/gi,
  )) {
    const start = match.index + match[0].length - match[1].length - match[2].length,
      end = match.index + match[0].length;
    if (!outside(start, end)) continue;
    const value = shortcutDate(match[2], now);
    if (value) groups[match[1] === '.' ? 'planned' : 'due'].push({ start, end, value });
  }
  // Keep established unprefixed natural-language due dates in global capture.
  if (naturalDates)
    for (const match of text.matchAll(
      /(?:^|\s)(today|tomorrow|in\s+\d+\s+days?|(?:next\s+)?(?:monday|tuesday|wednesday|thursday|friday|saturday|sunday))(?=\s|$|[,.!?])/gi,
    )) {
      const start = match.index + match[0].length - match[1].length,
        end = match.index + match[0].length;
      if (!outside(start, end)) continue;
      const value = shortcutDate(match[1].replace(/\s+/g, ''), now);
      if (value) groups.due.push({ start, end, value });
    }
  const result = { planned: null, due: null, spans, warnings };
  for (const [kind, candidates] of Object.entries(groups)) {
    if (candidates.length > 1) warnings.push(`Multiple ${kind} dates; choose one`);
    else if (candidates.length === 1) {
      const candidate = candidates[0];
      result[kind] = candidate.value;
      spans.push({
        ...candidate,
        kind,
        text: text.slice(candidate.start, candidate.end),
      });
    }
  }
  return result;
}
export function captureToday(timezone, now = new Date()) {
  if (!timezone) return now;
  const parts = new Intl.DateTimeFormat('en', {
    timeZone: timezone,
    year: 'numeric',
    month: 'numeric',
    day: 'numeric',
  }).formatToParts(now);
  const part = (type) => Number(parts.find((p) => p.type === type).value);
  return new Date(part('year'), part('month') - 1, part('day'), 12);
}
