import { captureDateSpans } from './capture-dates.mjs';
// Deterministic suggestions only. The caller must preserve the original input
// until the user has reviewed the interpretation and persisted the metadata.
const boundaries = /[\p{L}\p{N}_-]/u;
const fold = (s) => s.normalize('NFKC').toLocaleLowerCase();

export function parseCapture(
  text,
  {
    destinations = [],
    now = new Date(),
    dates = true,
    sections = [],
    naturalDates = true,
  } = {},
) {
  const spans = [],
    warnings = [],
    result = {
      original: text,
      title: text,
      destination: null,
      tags: [],
      priority: null,
      duration: null,
      due: null,
      planned: null,
      spans,
      warnings,
    };
  const exact = (a, b) => fold(a) === fold(b);
  const consume = (start, end, kind, value) =>
    spans.push({ start, end, kind, value, text: text.slice(start, end) });
  const tokens = [];
  for (let i = 0; i < text.length; i++) {
    const symbol = text[i];
    if (
      !'@#+$'.includes(symbol) ||
      (i && !/\s/.test(text[i - 1]) && !(symbol === '#' && tokens.at(-1)?.end === i))
    )
      continue;
    let end = i + 1,
      name;
    if (text[end] === '"') {
      const close = text.indexOf('"', end + 1);
      if (close < 0) {
        warnings.push('Unclosed quoted name');
        continue;
      }
      name = text.slice(end + 1, close);
      end = close + 1;
    } else {
      while (end < text.length && boundaries.test(text[end])) end++;
      name = text.slice(i + 1, end);
    }
    if (name) {
      tokens.push({ symbol, name, start: i, end });
      i = end - 1;
    }
  }
  const spaceTokens = tokens.filter((t) => t.symbol === '@'),
    listTokens = tokens.filter((t) => t.symbol === '#');
  const space = spaceTokens.length === 1 ? spaceTokens[0] : null;
  const list = listTokens.length === 1 ? listTokens[0] : null;
  if (spaceTokens.length > 1 || listTokens.length > 1)
    warnings.push('Multiple destinations; choose one');
  const spaceMatches = space
    ? [
        ...new Set(
          destinations.filter((d) => exact(d.space, space.name)).map((d) => d.space_id),
        ),
      ]
    : [];
  let matches = list
    ? destinations.filter(
        (d) =>
          (!space || (spaceMatches.length === 1 && d.space_id === spaceMatches[0])) &&
          exact(d.list, list.name),
      )
    : [];
  if (space && !list && spaceMatches.length === 1)
    matches = destinations.filter(
      (d) => d.space_id === spaceMatches[0] && d.default_list,
    );
  if ((space || list) && spaceTokens.length <= 1 && listTokens.length <= 1) {
    if (matches.length === 1) {
      result.destination = matches[0];
      if (space) consume(space.start, space.end, 'space', matches[0].space);
      if (list) consume(list.start, list.end, 'list', matches[0].list);
    } else warnings.push('Destination is unknown or ambiguous');
  }
  const sectionTokens = tokens.filter((t) => t.symbol === '$');
  if (sectionTokens.length === 1) {
    const token = sectionTokens[0];
    const matches = sections.filter((s) => exact(s.title, token.name));
    if (matches.length === 1) {
      result.section_id = matches[0].id;
      consume(token.start, token.end, 'section', token.name);
    }
  }
  for (const t of tokens.filter((t) => t.symbol === '+')) {
    result.tags.push(t.name);
    consume(t.start, t.end, 'tag', t.name);
  }
  const outsideTokens = (m) => {
    const start = m.index + m[0].length - m[1].length;
    const end = m.index + m[0].length;
    return !tokens.some((t) => start < t.end && end > t.start);
  };
  const priorities = [
    ...text.matchAll(/(?:^|\s)(p[0-3]|![0-3]|!{1,3})(?=\s|$)/gi),
  ].filter(outsideTokens);
  if (priorities.length === 1) {
    const m = priorities[0],
      token = m[1].toLowerCase();
    const value = /^!+$/.test(token) ? 3 - token.length : Number(token[1]);
    result.priority = value;
    consume(
      m.index + m[0].length - token.length,
      m.index + m[0].length,
      'priority',
      value,
    );
  } else if (priorities.length > 1) warnings.push('Multiple priorities; choose one');
  // Colons are explicit so times, URLs and ordinary prose remain literal.
  const outsideQuotes = (match) => {
    const start = match.index + match[0].length - match[1].length;
    let quoted = false;
    for (let i = 0; i < start; i++) {
      if (text[i] === '\\') {
        i++;
        continue;
      }
      if (text[i] === '"') quoted = !quoted;
    }
    return !quoted;
  };
  const durationMatches = [
    ...text.matchAll(
      /(?:^|\s)(:(?:0(?![\p{L}\p{N}_])|(?:\d+\s*(?:minutes?|mins?|m|hours?|hrs?|h|days?|d))(?:\s*\d+\s*(?:minutes?|mins?|m|hours?|hrs?|h|days?|d))*))(?=$|\s|[,.!?])/giu,
    ),
  ].filter((match) => outsideTokens(match) && outsideQuotes(match));
  if (durationMatches.length === 1) {
    const match = durationMatches[0];
    const token = match[1];
    let minutes = 0;
    for (const part of token
      .slice(1)
      .matchAll(/(\d+)\s*(minutes?|mins?|m|hours?|hrs?|h|days?|d)/gi)) {
      const unit = part[2].toLowerCase();
      minutes +=
        Number(part[1]) * (unit.startsWith('d') ? 1440 : unit.startsWith('h') ? 60 : 1);
    }
    if (Number.isSafeInteger(minutes) && minutes <= 5256000) {
      result.duration = minutes;
      consume(
        match.index + match[0].length - token.length,
        match.index + match[0].length,
        'duration',
        minutes,
      );
    } else warnings.push('Duration exceeds the supported limit');
  } else if (durationMatches.length > 1) warnings.push('Multiple durations; choose one');
  if (dates) {
    const parsedDates = captureDateSpans(text, { now, naturalDates, tokens });
    result.planned = parsedDates.planned;
    result.due = parsedDates.due;
    spans.push(...parsedDates.spans);
    warnings.push(...parsedDates.warnings);
  }
  let title = text;
  for (const span of [...spans].sort((a, b) => b.start - a.start))
    title = title.slice(0, span.start) + title.slice(span.end);
  result.title = title.replace(/\s+/g, ' ').trim();
  if (!result.title) {
    result.title = text;
    result.destination = null;
    result.section_id = undefined;
    result.tags = [];
    result.priority = null;
    result.duration = null;
    result.due = null;
    result.planned = null;
    spans.length = 0;
    warnings.push('Task title cannot be empty');
  }
  return result;
}
