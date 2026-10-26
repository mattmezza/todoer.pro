const STATIC_CACHE = 'todoer-static-v1';
const STATIC = ['/assets/app.css', '/assets/app.js', '/assets/offline.js', '/assets/htmx.min.js', '/legacy-assets/favicon.png'];
const scopes = new Map();
const pageCache = scope => `todoer-page-${scope}`;
self.addEventListener('install', event => {
  // An update waits until the UI explicitly asks to activate it.
  event.waitUntil(caches.open(STATIC_CACHE).then(cache => Promise.all(STATIC.map(async path => {
    try { const response = await fetch(path); if (response.ok) await cache.put(path, response); } catch { /* Next online fetch can fill it. */ }
  }))));
});
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('message', event => {
  if (event.data?.type === 'APPLY_UPDATE') { self.skipWaiting(); return; }
  const id = event.source?.id;
  if (!id) return;
  if (event.data?.type === 'SET_SCOPE' && typeof event.data.scope === 'string' && event.data.scope) {
    scopes.set(id, event.data.scope);
  }
  if (event.data?.type === 'CLEAR_SCOPE') {
    const scope = scopes.get(id) || event.data.scope;
    if (scope) {
      for (const [client, value] of scopes) if (value === scope) scopes.delete(client);
      event.waitUntil(caches.delete(pageCache(scope)));
    }
  }
});
const unavailable = () => new Response('<!doctype html><html lang="en"><meta charset="utf-8"><title>Offline</title><h1>You are offline</h1><p>This page has no saved offline copy. Reconnect and try again.</p></html>', { status: 503, headers: { 'Content-Type': 'text/html; charset=utf-8' } });
self.addEventListener('fetch', event => {
  const request = event.request;
  const url = new URL(request.url);
  if (request.method !== 'GET' || url.origin !== self.location.origin) return;
  if (STATIC.includes(url.pathname)) {
    event.respondWith((async () => {
      try { const response = await fetch(request); if (response.ok) { const cache = await caches.open(STATIC_CACHE); await cache.put(request, response.clone()); } return response; }
      catch { return (await caches.match(request)) || unavailable(); }
    })()); return;
  }
  if (request.mode !== 'navigate') return;
  event.respondWith((async () => {
    const scope = scopes.get(event.clientId);
    const eligible = scope && url.pathname.startsWith('/lists/');
    if (scope && event.resultingClientId) scopes.set(event.resultingClientId, scope);
    try {
      const response = await fetch(request);
      // Redirects can be sign-in screens and must never enter an account cache.
      if (eligible && scopes.get(event.clientId) === scope && response.ok && !response.redirected && response.headers.get('Content-Type')?.includes('text/html')) {
        const cache = await caches.open(pageCache(scope));
        await cache.delete(request); await cache.put(request, response.clone());
        const keys = await cache.keys();
        for (const old of keys.slice(0, Math.max(0, keys.length - 10))) await cache.delete(old);
      }
      return response;
    } catch {
      if (eligible && scopes.get(event.clientId) === scope) { const cache = await caches.open(pageCache(scope)); const saved = await cache.match(request); if (saved) return saved; }
      return unavailable();
    }
  })());
});
