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

export const apiPath = (kind, id) => `/${kind}/${encodeURIComponent(id)}`;

export class ApiError extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}

export async function request(
  { origin, token },
  path,
  { body, key, method = 'GET', signal } = {},
) {
  const url = new URL(`/api/v1${path}`, origin);
  const headers = { Accept: 'application/json', Authorization: `Bearer ${token}` };
  if (body) headers['Content-Type'] = 'application/json';
  if (key) headers['Idempotency-Key'] = key;
  let response;
  try {
    response = await fetch(url, {
      method,
      headers,
      body: JSON.stringify(body),
      credentials: 'omit',
      redirect: 'error',
      cache: 'no-store',
      signal: signal ?? AbortSignal.timeout(15000),
    });
  } catch (error) {
    if (error.name === 'AbortError') throw error;
    throw new Error(
      'Could not reach the Todoer API. Check the server address, browser access and connection.',
    );
  }
  if (!response.ok) {
    const result = await response.json().catch(() => null);
    const message = {
      401: 'The credential has expired or was revoked. Open Settings to reconnect.',
      403: 'The credential does not have access for this action. Check its lists and permissions in Settings.',
    }[response.status];
    throw new ApiError(
      response.status,
      message || result?.error?.message || `Todoer returned HTTP ${response.status}.`,
    );
  }
  try {
    return await response.json();
  } catch {
    throw new Error('The server did not return a Todoer API response.');
  }
}

export async function getLists(connection) {
  const lists = await request(connection, '/lists');
  if (!Array.isArray(lists))
    throw new Error('The server did not return a Todoer list response.');
  return lists;
}

export function preferenceKey(connection, name) {
  return `todoer:${connection.id}:${name}`;
}
