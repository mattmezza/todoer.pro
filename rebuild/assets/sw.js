// Bump this build version whenever precached asset contents change.
const STATIC_CACHE = 'todoer-static-__TODOER_BUILD__';
const META_CACHE = 'todoer-offline-meta-v1';
const SCOPE_KEY = '/__todoer_offline_scope__';
let scopeWrites = Promise.resolve();
async function persistedScope() {
  const cache = await caches.open(META_CACHE);
  const saved = await cache.match(SCOPE_KEY);
  return saved ? saved.text() : null;
}
async function currentScope(clientId) {
  await scopeWrites;
  return scopes.get(clientId) || (await persistedScope());
}
const STATIC = [
  '/assets/app.css',
  '/assets/app.js',
  '/assets/discussion.js',
  '/assets/push.js',
  '/assets/scheduling.js',
  '/assets/task-metadata.js',
  '/assets/task-order.js',
  '/assets/offline.js',
  '/assets/pwa.js',
  '/assets/manifest.webmanifest',
  '/assets/htmx.min.js',
  '/legacy-assets/favicon.png',
];
const scopes = new Map();
const pageCache = (scope) => `todoer-page-${scope}`;
async function savePage(scope, request, response) {
  const url = new URL(typeof request === 'string' ? request : request.url);
  if (
    url.origin !== self.location.origin ||
    !url.pathname.startsWith('/lists/') ||
    !response.ok ||
    response.redirected ||
    response.headers.get('X-Todoer-Account') !== scope ||
    !response.headers.get('Content-Type')?.includes('text/html') ||
    (await persistedScope()) !== scope
  )
    return;
  const cache = await caches.open(pageCache(scope));
  await cache.delete(request);
  await cache.put(request, response.clone());
  // Logout or switching accounts may have happened while writing the cache.
  if ((await persistedScope()) !== scope) {
    await caches.delete(pageCache(scope));
    return;
  }
  const keys = await cache.keys();
  for (const old of keys.slice(0, Math.max(0, keys.length - 10))) await cache.delete(old);
}
self.addEventListener('install', (event) => {
  // An update waits until the UI explicitly asks to activate it.
  event.waitUntil(
    caches.open(STATIC_CACHE).then((cache) =>
      Promise.all(
        STATIC.map(async (path) => {
          try {
            const response = await fetch(path);
            if (response.ok) await cache.put(path, response);
          } catch {
            /* Next online fetch can fill it. */
          }
        }),
      ),
    ),
  );
});
self.addEventListener('activate', (event) =>
  event.waitUntil(
    (async () => {
      for (const name of await caches.keys()) {
        if (name.startsWith('todoer-static-') && name !== STATIC_CACHE)
          await caches.delete(name);
      }
      await self.clients.claim();
    })(),
  ),
);
self.addEventListener('message', (event) => {
  if (event.data?.type === 'APPLY_UPDATE') {
    self.skipWaiting();
    return;
  }
  const id = event.source?.id;
  if (!id) return;
  if (
    event.data?.type === 'SET_SCOPE' &&
    typeof event.data.scope === 'string' &&
    event.data.scope
  ) {
    const scope = event.data.scope;
    scopeWrites = scopeWrites
      .catch(() => {})
      .then(async () => {
        const previous = await persistedScope();
        if (previous !== scope) {
          scopes.clear();
          // There is one active local account; switching removes older page copies.
          for (const name of await caches.keys())
            if (name.startsWith('todoer-page-')) await caches.delete(name);
        }
        scopes.set(id, scope);
        const cache = await caches.open(META_CACHE);
        await cache.put(
          SCOPE_KEY,
          new Response(scope, { headers: { 'Content-Type': 'text/plain' } }),
        );
      });
    event.waitUntil(
      scopeWrites.then(async () => {
        try {
          const url = new URL(event.source.url);
          if (url.origin === self.location.origin && url.pathname.startsWith('/lists/')) {
            const response = await fetch(url.href, {
              credentials: 'same-origin',
              cache: 'no-store',
            });
            await savePage(scope, url.href, response);
          }
        } catch {
          /* Keep existing recent pages when offline. */
        }
      }),
    );
  }
  if (event.data?.type === 'CLEAR_SCOPE') {
    scopeWrites = scopeWrites
      .catch(() => {})
      .then(async () => {
        scopes.clear();
        await caches.delete(META_CACHE);
        for (const name of await caches.keys())
          if (name.startsWith('todoer-page-')) await caches.delete(name);
      });
    event.waitUntil(scopeWrites);
  }
});
const unavailable = () =>
  new Response(
    '<!doctype html><html lang="en"><meta charset="utf-8"><title>Offline</title><h1>You are offline</h1><p>This page has no saved offline copy. Reconnect and try again.</p></html>',
    { status: 503, headers: { 'Content-Type': 'text/html; charset=utf-8' } },
  );
self.addEventListener('fetch', (event) => {
  const request = event.request;
  const url = new URL(request.url);
  if (request.method !== 'GET' || url.origin !== self.location.origin) return;
  if (STATIC.includes(url.pathname)) {
    event.respondWith(
      (async () => {
        try {
          const response = await fetch(request);
          if (response.ok) {
            const cache = await caches.open(STATIC_CACHE);
            await cache.put(request, response.clone());
          }
          return response;
        } catch {
          return (
            (await (await caches.open(STATIC_CACHE)).match(request)) || unavailable()
          );
        }
      })(),
    );
    return;
  }
  if (request.mode !== 'navigate') return;
  event.respondWith(
    (async () => {
      const scope = await currentScope(event.clientId);
      const eligible = scope && url.pathname.startsWith('/lists/');
      if (scope && event.resultingClientId) scopes.set(event.resultingClientId, scope);
      try {
        const response = await fetch(request);
        // Redirects can be sign-in screens and must never enter an account cache.
        if (eligible && (await currentScope(event.clientId)) === scope)
          await savePage(scope, request, response);
        return response;
      } catch {
        if (eligible && (await currentScope(event.clientId)) === scope) {
          const cache = await caches.open(pageCache(scope));
          const saved = await cache.match(request);
          if (saved) return saved;
        }
        return unavailable();
      }
    })(),
  );
});

self.addEventListener('push', (event) => {
  event.waitUntil(
    (async () => {
      // Payloads deliberately contain no task title or brief. A session-scoped
      // subscription is disabled server-side at logout; stale local accounts
      // must not display notifications received after an account switch.
      if (!(await persistedScope())) return;
      let payload;
      try {
        payload = event.data?.json();
      } catch {
        return;
      }
      if (!payload || !notificationPath(payload.url)) return;
      const summary = payload.url === '/review';
      await self.registration.showNotification(
        summary ? 'Todoer summary' : 'Todoer reminder',
        {
          body: summary
            ? 'Open Todoer for your review.'
            : 'Open Todoer to review your reminder.',
          icon: '/legacy-assets/favicon.png',
          badge: '/legacy-assets/favicon.png',
          tag:
            typeof payload.intent_id === 'string'
              ? payload.intent_id.slice(0, 150)
              : undefined,
          data: { url: payload.url },
        },
      );
    })(),
  );
});
self.addEventListener('notificationclick', (event) => {
  event.notification.close();
  event.waitUntil(
    (async () => {
      const path = event.notification.data?.url;
      if (!notificationPath(path) || !(await persistedScope())) return;
      const url = new URL(path, self.location.origin).href;
      for (const client of await self.clients.matchAll({
        type: 'window',
        includeUncontrolled: true,
      })) {
        if (new URL(client.url).origin !== self.location.origin) continue;
        await client.navigate(url);
        await client.focus();
        return;
      }
      await self.clients.openWindow(url);
    })(),
  );
});

function notificationPath(path) {
  return (
    path === '/review' ||
    (typeof path === 'string' && /^\/tasks\/[a-zA-Z0-9-]+$/.test(path))
  );
}
