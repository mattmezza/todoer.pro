export const browserApi = globalThis.browser ?? globalThis.chrome;
export const storage = browserApi.storage.local;
export const supportsSidePanel = Boolean(
  browserApi.sidePanel?.open || browserApi.sidebarAction?.open,
);
export const cloudOrigin = 'https://app.todoer.pro';
export const inboxPath = '/inbox';

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
export const entrypointKey = (origin) => `entrypoint:${origin}`;

export function sitePattern(value) {
  const pattern = value.trim();
  if (pattern === '*') return pattern;
  const wildcard = pattern.startsWith('*.');
  const name = wildcard ? pattern.slice(2) : pattern;
  const hostname = siteHost('https://' + name);
  if (
    !name ||
    /[\s/\\#?@:%*]/.test(name) ||
    !hostname
      .split('.')
      .every((label) => /^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$/i.test(label))
  ) {
    throw new Error(
      'Enter a hostname or wildcard, such as example.com, *.example.com, or *.',
    );
  }
  return wildcard ? '*.' + hostname : hostname;
}

export function todoerPath(value, origin) {
  let url;
  try {
    url = new URL(value.trim());
  } catch {
    throw new Error('Paste a URL from your Todoer server.');
  }
  if (
    !['https:', 'http:'].includes(url.protocol) ||
    url.origin !== origin ||
    url.username ||
    url.password
  ) {
    throw new Error('Paste a URL from your Todoer server.');
  }
  return url.href.slice(origin.length);
}

export function savedDestination(origin, path) {
  if (typeof path !== 'string' || !path.startsWith('/')) return undefined;
  return new URL(origin + path);
}

export function listId(destination) {
  return /^\/lists\/([a-z0-9_-]+)\/?$/i.exec(destination?.pathname)?.[1];
}

export async function pinnedDestination(origin, hostname) {
  if (!hostname) return undefined;
  const labels = hostname.split('.');
  const patterns = [
    hostname,
    ...labels.slice(1).map((_, i) => '*.' + labels.slice(i + 1).join('.')),
    '*',
  ];
  const keys = patterns.map((pattern) => pinPrefix(origin) + pattern);
  const pins = await storage.get(keys);
  for (const key of keys) {
    const destination = savedDestination(origin, pins[key]);
    if (destination) return destination;
  }
}

export async function defaultDestination(origin) {
  const key = entrypointKey(origin);
  const entrypoint = (await storage.get(key))[key];
  return savedDestination(origin, entrypoint) ?? new URL(inboxPath, origin);
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
