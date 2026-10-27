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
    timeZoneName: 'short',
  });
  return (timestamp) =>
    timestamp == null ? 'Time unavailable' : formatter.format(new Date(timestamp * 1000));
}
