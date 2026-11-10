// Execute the actual worker handlers with browser API fixtures. This checks
// payload/privacy boundaries; it does not simulate device push delivery.
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const build = 'a'.repeat(64);
const workerSource = (await readFile('app/assets/sw.js', 'utf8')).replaceAll('__TODOER_BUILD__', build);
const assetBase = `/static/${build}/`;
const handlers = new Map();
const notices = [], opened = [];
let scope = 'synthetic-user';
const context = vm.createContext({
  URL, Promise, Map, Response,
  caches: { open: async () => ({ match: async () => scope ? new Response(scope) : undefined }) },
  self: {
    location: { origin: 'https://tasks.example.invalid' },
    addEventListener: (name, fn) => handlers.set(name, fn),
    registration: { showNotification: async (title, options) => notices.push({ title, ...options }) },
    clients: { matchAll: async () => [], openWindow: async (url) => opened.push(url) },
  },
});
vm.runInContext(workerSource, context);
// Every static import needed by the shared UI must be available before offline
// navigation, including components whose settings page has never been visited.
const precached = new Set(vm.runInContext('STATIC', context));
const moduleQueue = ['/assets/alpine.js'];
const visitedModules = new Set();
while (moduleQueue.length) {
  const asset = moduleQueue.pop();
  if (visitedModules.has(asset)) continue;
  visitedModules.add(asset);
  assert.ok(precached.has(asset.replace('/assets/', assetBase)), `Alpine dependency missing from precache: ${asset}`);
  const source = await readFile(`app${asset}`, 'utf8');
  for (const match of source.matchAll(/(?:from\s*|import\s*)['"](\.\/[^'"]+)['"]/g))
    moduleQueue.push(new URL(match[1], `https://tasks.example.invalid${asset}`).pathname);
}
console.log(`Service-worker Alpine dependencies: all ${visitedModules.size} modules precached`);
async function push(payload) {
  let work;
  handlers.get('push')({ data: { json: () => payload }, waitUntil: value => { work = value; } });
  await work;
}
async function click(url) {
  let work;
  handlers.get('notificationclick')({ notification: { close() {}, data: { url } }, waitUntil: value => { work = value; } });
  await work;
}
await push({ url: '/review', title: 'Private title must not display', body: 'Private body', intent_id: 'digest-intent' });
assert.equal(notices.length, 1);
assert.equal(notices[0].title, 'Todoer summary');
assert.equal(notices[0].body, 'Open Todoer for your review.');
assert.equal(notices[0].tag, 'digest-intent');
await push({ url: '/tasks/abcdef-1234', intent_id: 'reminder-intent' });
assert.equal(notices[1].title, 'Todoer reminder');
for (const url of ['https://evil.invalid/review', '//evil.invalid', '/review?secret=yes', '/tasks/../settings', '/settings', null]) {
  await push({ url });
  await click(url);
}
assert.equal(notices.length, 2);
assert.equal(opened.length, 0);
await click('/review');
assert.equal(opened[0], 'https://tasks.example.invalid/review');
scope = null;
await push({ url: '/review' });
await click('/review');
assert.equal(notices.length, 2);
assert.equal(opened.length, 1);
console.log('Actual service-worker handlers: generic summaries/reminders, strict routes and logged-out suppression passed');

// Full-page HTMX GETs use the same account-isolated offline cache as navigation.
const pageHandlers = new Map();
const stores = new Map();
let pageScope = 'synthetic-page-user';
let offline = false;
let redirectedURL;
const keyOf = (request) => typeof request === 'string' ? request : request.url;
const cacheStore = (name) => {
  if (!stores.has(name)) stores.set(name, new Map());
  const entries = stores.get(name);
  return {
    match: async (request) => {
      if (name === 'todoer-offline-meta-v1') return pageScope ? new Response(pageScope) : undefined;
      return entries.get(keyOf(request))?.clone();
    },
    put: async (request, response) => entries.set(keyOf(request), response),
    delete: async (request) => entries.delete(keyOf(request)),
    keys: async () => [...entries.keys()],
  };
};
const pageContext = vm.createContext({
  URL, Promise, Map, Response,
  caches: { open: async (name) => cacheStore(name), delete: async (name) => stores.delete(name) },
  fetch: async () => {
    if (offline) throw Error('Offline fixture');
    const response = new Response('<main id="main">Synthetic offline page</main>', {
      headers: { 'Content-Type': 'text/html', 'X-Todoer-Account': pageScope, 'X-Todoer-Build': build },
    });
    if (redirectedURL) Object.defineProperties(response, {
      redirected: { value: true }, url: { value: redirectedURL },
    });
    return response;
  },
  self: {
    location: { origin: 'https://tasks.example.invalid' },
    addEventListener: (name, fn) => pageHandlers.set(name, fn),
  },
});
vm.runInContext(workerSource, pageContext);
async function pageFetch(path, headers = {}, mode = 'same-origin') {
  let response;
  const work = [];
  pageHandlers.get('fetch')({
    request: { url: `https://tasks.example.invalid${path}`, method: 'GET', mode, headers: new Headers(headers) },
    clientId: 'synthetic-client',
    respondWith: (value) => { response = value; },
    waitUntil: (value) => work.push(value),
  });
  const result = await response;
  await Promise.all(work);
  return result;
}
const fullPageHeaders = { 'HX-Request': 'true', 'HX-Request-Type': 'full' };
assert.equal((await pageFetch('/lists/example', fullPageHeaders)).status, 200);
offline = true;
assert.match(await (await pageFetch('/lists/example', fullPageHeaders)).text(), /Synthetic offline page/);
assert.match(await (await pageFetch('/lists/example', {}, 'navigate')).text(), /Synthetic offline page/);
assert.equal(await pageFetch('/fragments/lists/example/tasks', { 'HX-Request': 'true', 'HX-Request-Type': 'partial' }), undefined);
pageScope = 'another-synthetic-user';
assert.equal((await pageFetch('/lists/example', fullPageHeaders)).status, 503);
pageScope = null;
assert.equal((await pageFetch('/lists/example', fullPageHeaders)).status, 503);
pageScope = 'redirect-synthetic-user';
offline = false;
redirectedURL = 'https://tasks.example.invalid/lists/canonical';
const redirect = await pageFetch('/lists/internal-id', fullPageHeaders);
assert.equal(redirect.status, 303);
assert.equal(redirect.headers.get('Location'), redirectedURL);
offline = true;
assert.equal((await pageFetch('/lists/internal-id', fullPageHeaders)).status, 503);
console.log('Service-worker page requests: HTMX/navigation cache parity, fragment exclusion and account isolation passed');

// A warm immutable asset must resolve even while the network never answers.
const assetPath = `${assetBase}task-page.js`;
const assetURL = `https://tasks.example.invalid${assetPath}`;
await cacheStore(`todoer-static-${build}`).put(assetURL, new Response('cached module'));
let networkRequests = 0;
pageContext.fetch = () => { networkRequests++; return new Promise(() => {}); };
assert.equal(await (await pageFetch(assetPath)).text(), 'cached module');
assert.equal(networkRequests, 0, 'warm assets must not wait on mobile-network RTT');

// A cold asset is filled once, then served locally; failed/wrong-build responses
// never poison the immutable cache.
const coldPath = `${assetBase}cold.js`;
pageContext.fetch = async () => {
  networkRequests++;
  return new Response('cold module', { headers: { 'X-Todoer-Build': build } });
};
assert.equal(await (await pageFetch(coldPath)).text(), 'cold module');
assert.equal(await (await pageFetch(coldPath)).text(), 'cold module');
assert.equal(networkRequests, 1);
pageContext.fetch = async () => new Response('wrong build', { headers: { 'X-Todoer-Build': 'b'.repeat(64) } });
await pageFetch(`${assetBase}wrong.js`);
assert.equal(await cacheStore(`todoer-static-${build}`).match(`https://tasks.example.invalid${assetBase}wrong.js`), undefined);

// Cached offline documents are private to both the account and the release.
vm.runInContext(`scopes.set('synthetic-client', 'release-user')`, pageContext);
pageScope = 'release-user';
pageContext.fetch = async () => new Response('new release page', {
  headers: { 'Content-Type': 'text/html', 'X-Todoer-Account': pageScope, 'X-Todoer-Build': 'b'.repeat(64) },
});
await pageFetch('/lists/new-release', fullPageHeaders);
pageContext.fetch = async () => { throw Error('Offline'); };
assert.equal((await pageFetch('/lists/new-release', fullPageHeaders)).status, 503);
console.log('Service-worker immutable assets: zero-network warm reads, cold fills and release isolation passed');
