// Execute the actual worker handlers with browser API fixtures. This checks
// payload/privacy boundaries; it does not simulate device push delivery.
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';
import assert from 'node:assert/strict';
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
vm.runInContext(await readFile('rebuild/assets/sw.js', 'utf8'), context);
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
