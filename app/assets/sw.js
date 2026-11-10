// The build hash includes every asset (including fonts) and the release version.
const BUILD = '__TODOER_BUILD__';
const ASSET_BASE = `/static/${BUILD}/`;
const STATIC_CACHE = `todoer-static-${BUILD}`;
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
  '/assets/review.css',
  '/assets/account-preferences.js',
  '/assets/account-auth.js',
  '/assets/account-security.js',
  '/assets/admin-email.js',
  '/assets/ai-account-picker.css',
  '/assets/ai-billing.css',
  '/assets/ai-billing.js',
  '/assets/ai-transaction.js',
  '/assets/cli-consent.js',
  '/assets/delivery-settings.js',
  '/assets/passkeys.js',
  '/assets/planning-workbench.js',
  '/assets/public-sharing.js',
  '/assets/quick-capture.js',
  '/assets/review.js',
  '/assets/sharing-settings.css',
  '/assets/task-structure.js',
  '/assets/app.css',
  '/assets/numeric-mono.woff2',
  '/assets/app.js',
  '/assets/site.js',
  '/assets/alpine.js',
  '/assets/about-dialog.js',
  '/assets/menu.js',
  '/assets/dialog.js',
  '/assets/confirmation-dialog.js',
  '/assets/export-dialog.js',
  '/assets/workspace-dialogs.js',
  '/assets/alpine-csp.min.js',
  '/assets/list-create.js',
  '/assets/list-preferences.js',
  '/assets/capture-preferences.js',
  '/assets/page-lifecycle.js',
  '/assets/page-navigation.js',
  '/assets/navigation.js',
  '/assets/home-filters.js',
  '/assets/section-collapse.js',
  '/assets/section-order.js',
  '/assets/task-panel.js',
  '/assets/dom-ids.js',
  '/assets/task-gestures.js',
  '/assets/task-swipe-actions.js',
  '/assets/task-indicators.js',
  '/assets/task-list-refresh.js',
  '/assets/structured-edit.js',
  '/assets/settings-feedback.js',
  '/assets/export-menu.js',
  '/assets/list-archive.js',
  '/assets/entity-name.js',
  '/assets/reference-copy.js',
  '/assets/favorite-order.js',
  '/assets/nav-overflow.js',
  '/assets/loading.js',
  '/assets/request.js',
  '/assets/reauth.js',
  '/assets/notification-center.js',
  '/assets/task-play.js',
  '/assets/planning-page.js',
  '/assets/general-settings.js',
  '/assets/space-overview.js',
  '/assets/space-list-order.js',
  '/assets/notification-picker.js',
  '/assets/activity-panel.js',
  '/assets/preferred-time.js',
  '/assets/search-omnibar.js',
  '/assets/search-dialog.js',
  '/assets/confirm.js',
  '/assets/task-completion.js',
  '/assets/toast.js',
  '/assets/quiet-info.js',
  '/assets/capture-ui.js',
  '/assets/capture-page.js',
  '/assets/smart-capture.js',
  '/assets/capture-help.js',
  '/assets/smart-capture-parser.mjs',
  '/assets/capture-dates.mjs',
  '/assets/list-ui.js',
  '/assets/list-color-picker.js',
  '/assets/list-settings.js',
  '/assets/workspaces.js',
  '/assets/icons.svg',
  '/assets/task-page.js',
  '/assets/task-breakdown.js',
  '/assets/icons.svg',
  '/assets/discussion.js',
  '/assets/push.js',
  '/assets/scheduling.js',
  '/assets/task-metadata.js',
  '/assets/task-order.js',
  '/assets/section-order.js',
  '/assets/offline.js',
  '/assets/pwa.js',
  '/assets/welcome.js',
  '/assets/welcome-chase.js',
  '/assets/manifest.webmanifest',
  '/assets/icon192-v2.png',
  '/assets/notification-badge-v2.png',
  '/assets/apple-touch-icon-v2.png',
  '/assets/icon512-v2.png',
  '/assets/icon512-maskable-v2.png',
  '/assets/htmx.min.js',
  '/assets/favicon.svg?v=2',
  '/favicon.ico?v=2',
].map((path) =>
  /\.(?:m?js|css|woff2)$/.test(path) ? ASSET_BASE + path.slice('/assets/'.length) : path,
);
const scopes = new Map();
const pageCache = (scope) => `todoer-page-${BUILD}-${scope}`;
async function savePage(scope, request, response) {
  const url = new URL(typeof request === 'string' ? request : request.url);
  if (
    url.origin !== self.location.origin ||
    !url.pathname.startsWith('/lists/') ||
    !response.ok ||
    response.redirected ||
    response.headers.get('X-Todoer-Account') !== scope ||
    response.headers.get('X-Todoer-Build') !== BUILD ||
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
  // An update waits until the UI explicitly asks to activate it. Limit warming
  // to three concurrent requests so a slow mobile connection can serve the UI.
  event.waitUntil(
    (async () => {
      const cache = await caches.open(STATIC_CACHE);
      for (let offset = 0; offset < STATIC.length; offset += 3) {
        await Promise.all(
          STATIC.slice(offset, offset + 3).map(async (path) => {
            try {
              // Immutable URLs reuse the browser's HTTP cache after the first visit.
              const response = await fetch(path);
              if (response.ok && response.headers.get('X-Todoer-Build') === BUILD)
                await cache.put(path, response);
            } catch {
              /* Next online fetch can fill it. */
            }
          }),
        );
      }
    })(),
  );
});
self.addEventListener('activate', (event) =>
  event.waitUntil(
    (async () => {
      const names = await caches.keys();
      // Keep the preceding build for tabs still finishing work with old modules.
      const previous = names
        .filter((name) => name.startsWith('todoer-static-') && name !== STATIC_CACHE)
        .at(-1);
      for (const name of names) {
        if (
          name.startsWith('todoer-static-') &&
          name !== STATIC_CACHE &&
          name !== previous
        )
          await caches.delete(name);
        if (name.startsWith('todoer-page-') && !name.startsWith(`todoer-page-${BUILD}-`))
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
  const assetBuild = url.pathname.match(
    /^\/static\/([a-f0-9]{64}|__TODOER_BUILD__)\//,
  )?.[1];
  if (assetBuild) {
    // Versioned assets never need a network round trip on a warm cache.
    event.respondWith(
      (async () => {
        const name = `todoer-static-${assetBuild}`;
        const cache = await caches.open(name);
        const saved = await cache.match(request);
        if (saved) return saved;
        try {
          const response = await fetch(request);
          if (response.ok && response.headers.get('X-Todoer-Build') === assetBuild)
            event.waitUntil(cache.put(request, response.clone()).catch(() => {}));
          return response;
        } catch {
          return unavailable();
        }
      })(),
    );
    return;
  }
  if (STATIC.includes(url.pathname) || STATIC.includes(url.pathname + url.search)) {
    // Public, unversioned installation metadata must still revalidate on releases.
    event.respondWith(
      (async () => {
        const cache = await caches.open(STATIC_CACHE);
        try {
          const response = await fetch(request);
          if (response.ok)
            event.waitUntil(cache.put(request, response.clone()).catch(() => {}));
          return response;
        } catch {
          return (await cache.match(request)) || unavailable();
        }
      })(),
    );
    return;
  }
  const pageRequest =
    request.mode === 'navigate' ||
    (request.headers.get('HX-Request') === 'true' &&
      request.headers.get('HX-Request-Type') === 'full' &&
      url.pathname.startsWith('/lists/'));
  if (!pageRequest) return;
  event.respondWith(
    (async () => {
      const scope = await currentScope(event.clientId);
      const eligible = scope && url.pathname.startsWith('/lists/');
      if (scope && event.resultingClientId) scopes.set(event.resultingClientId, scope);
      try {
        const response = await fetch(request);
        // Redirects can be sign-in screens and must never enter an account cache.
        if (
          eligible &&
          !response.redirected &&
          (await currentScope(event.clientId)) === scope
        )
          event.waitUntil(savePage(scope, request, response.clone()).catch(() => {}));
        // Forward a fresh redirect; a followed response can stall HTMX fetches in Firefox.
        if (response.redirected && request.headers.get('HX-Request') === 'true')
          return Response.redirect(response.url, 303);
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
      const inbox = payload.url === '/notifications';
      await self.registration.showNotification(
        inbox ? 'Todoer notification' : summary ? 'Todoer summary' : 'Todoer reminder',
        {
          body: inbox
            ? 'Open Todoer for your notifications.'
            : summary
              ? 'Open Todoer for your review.'
              : 'Open Todoer to review your reminder.',
          icon: '/assets/icon192-v2.png',
          badge: '/assets/notification-badge-v2.png',
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
    path === '/notifications' ||
    path === '/review' ||
    (typeof path === 'string' && /^\/tasks\/[a-zA-Z0-9-]+$/.test(path))
  );
}
