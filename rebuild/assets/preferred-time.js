export async function preferredTimeFormatter() {
  let timeZone = 'UTC';
  try {
    const response = await fetch('/api/v1/notification-preferences', {
      cache: 'no-store',
    });
    if (response.ok) timeZone = (await response.json()).timezone || 'UTC';
  } catch {}
  const formatter = new Intl.DateTimeFormat(undefined, {
    timeZone,
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
    second: '2-digit',
  });
  const format = (timestamp) =>
    timestamp == null ? 'Time unavailable' : formatter.format(new Date(timestamp * 1000));
  format.timeZone = timeZone;
  return format;
}

// Resolve an existing validated wall time before formatting in account time.
export function zonedLocalTimestamp(local, timeZone) {
  const wall = Date.parse(local + 'Z');
  if (!Number.isFinite(wall)) return null;
  const parts = new Intl.DateTimeFormat('en-US', {
    timeZone,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hourCycle: 'h23',
  });
  let instant = wall;
  for (let i = 0; i < 4; i++) {
    const values = Object.fromEntries(
      parts.formatToParts(new Date(instant)).map((p) => [p.type, p.value]),
    );
    const shown = Date.UTC(
      Number(values.year),
      Number(values.month) - 1,
      Number(values.day),
      Number(values.hour),
      Number(values.minute),
      Number(values.second),
    );
    const correction = wall - shown;
    if (!correction) return instant / 1000;
    instant += correction;
  }
  return null;
}
export async function hydratePreferredTimes(root = document) {
  const nodes = [...root.querySelectorAll('[data-unix-time]')];
  if (!nodes.length) return;
  const format = await preferredTimeFormatter();
  for (const node of nodes) {
    const at = Number(node.dataset.unixTime);
    if (!Number.isFinite(at)) continue;
    node.textContent = format(at);
    node.title = format.timeZone;
    node.dateTime = new Date(at * 1000).toISOString();
  }
}
