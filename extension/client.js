export const browserApi = globalThis.browser ?? globalThis.chrome;
export const storage = browserApi.storage.local;
export const cloudOrigin = 'https://app.todoer.pro';

export function serverOrigin(value) {
  let url;
  try {
    url = new URL(value.trim());
  } catch {
    throw new Error('Enter the full Todoer server address, including https://.');
  }
  const local = ['localhost', '127.0.0.1'].includes(url.hostname);
  if (
    (url.protocol !== 'https:' && !(local && url.protocol === 'http:')) ||
    url.username ||
    url.password ||
    url.pathname !== '/' ||
    url.search ||
    url.hash
  ) {
    throw new Error('Enter an HTTPS server origin, such as https://app.todoer.pro.');
  }
  return url.origin;
}

export function siteHost(value) {
  try {
    const url = new URL(value);
    return ['https:', 'http:'].includes(url.protocol)
      ? url.hostname.replace(/\.$/, '')
      : '';
  } catch {
    return '';
  }
}

export const pinPrefix = (origin) => `pin:${origin}:`;

export async function pinnedPath(origin, hostname) {
  if (!hostname) return undefined;
  const key = pinPrefix(origin) + hostname;
  const path = (await storage.get(key))[key];
  if (typeof path === 'string' && /^\/lists\/[a-z0-9_-]+$/i.test(path)) return path;
}

export function listPath(value, origin) {
  const url = new URL(value);
  const match = /^\/lists\/([a-z0-9_-]+)\/?$/i.exec(url.pathname);
  if (url.origin !== origin || url.username || url.password || !match) {
    throw new Error('Paste a list URL from your Todoer server.');
  }
  return `/lists/${match[1]}`;
}

export async function settings() {
  return navigator.locks.request('todoer-settings', migrateSettings);
}

async function migrateSettings() {
  const { server, connection } = await storage.get(['server', 'connection']);
  if (!connection) return server ?? cloudOrigin;
  const origin = serverOrigin(connection.origin);
  const saved = await storage.get(null);
  const previous = `todoer:${connection.id}:`;
  const update = { server: server ?? origin };
  const pending = saved[previous + 'pending'];
  if (typeof pending?.title === 'string' && pending.title && !saved.previousDraft) {
    update.previousDraft = { origin, title: pending.title, list: pending.listId ?? '' };
  }
  for (const [key, id] of Object.entries(saved)) {
    if (!key.startsWith(`${previous}pin:`) || typeof id !== 'string') continue;
    const pin = pinPrefix(origin) + key.slice(previous.length + 4);
    if (pin in saved || !/^[a-z0-9_-]+$/i.test(id)) continue;
    update[pin] = `/lists/${id}`;
  }
  await storage.set(update);
  await storage.remove([
    'connection',
    ...Object.keys(saved).filter((key) => key.startsWith(previous)),
  ]);
  return update.server;
}
