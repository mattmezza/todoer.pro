import {
  listPath,
  pinPrefix,
  serverOrigin,
  settings,
  siteHost,
  storage,
} from './client.js';

const $ = (id) => document.getElementById(id);
const originInput = $('origin');
const siteInput = $('site');
const listInput = $('list-url');
let origin;

function status(message, error = false) {
  $('status').textContent = message;
  $('status').classList.toggle('error', error);
}

function hostname(value) {
  const name = value.trim();
  if (!name || /[\s/#?@:]/.test(name))
    throw new Error('Enter a website hostname, such as example.com.');
  const host = siteHost('https://' + name);
  if (!host) throw new Error('Enter a website hostname, such as example.com.');
  return host;
}

async function renderPins() {
  const prefix = pinPrefix(origin);
  const pins = Object.entries(await storage.get(null))
    .filter(([key]) => key.startsWith(prefix))
    .sort(([a], [b]) => a.localeCompare(b));
  $('pins').replaceChildren();
  for (const [key, path] of pins) {
    const host = key.slice(prefix.length);
    const item = document.createElement('li');
    const site = document.createElement('span');
    site.textContent = host;
    const list = document.createElement('a');
    list.href = new URL(listPath(origin + path, origin), origin);
    list.textContent = 'Open list';
    list.target = '_blank';
    list.rel = 'noopener';
    list.setAttribute('aria-label', `Open list pinned to ${host}`);
    const remove = document.createElement('button');
    remove.textContent = 'Unpin';
    remove.setAttribute('aria-label', `Unpin ${host}`);
    remove.addEventListener('click', async () => {
      try {
        await storage.remove(key);
        await renderPins();
        status(`Unpinned ${host}.`);
      } catch (error) {
        status(error.message, true);
      }
    });
    item.append(site, list, remove);
    $('pins').append(item);
  }
}

function showServer() {
  originInput.value = origin;
  $('open-todoer').href = origin + '/inbox';
  listInput.placeholder = origin + '/lists/…';
}

async function showPreviousDraft() {
  const { previousDraft } = await storage.get('previousDraft');
  if (!previousDraft) return;
  const url = new URL('/capture', previousDraft.origin);
  if (previousDraft.list) url.searchParams.set('list', previousDraft.list);
  url.hash =
    'todoer-capture&' + new URLSearchParams({ title: previousDraft.title }).toString();
  $('review-draft').href = url.href;
  $('previous-draft').hidden = false;
}

$('dismiss-draft').addEventListener('click', async () => {
  await storage.remove('previousDraft');
  $('previous-draft').hidden = true;
});

$('server-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  try {
    const next = serverOrigin(originInput.value);
    await storage.set({ server: next });
    origin = next;
    showServer();
    await renderPins();
    status('Todoer server saved.');
  } catch (error) {
    status(error.message, true);
  }
});

$('pin-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  try {
    const host = hostname(siteInput.value);
    await storage.set({ [pinPrefix(origin) + host]: listPath(listInput.value, origin) });
    await renderPins();
    status(`Saved the pin for ${host}.`);
  } catch (error) {
    status(error.message, true);
  }
});

try {
  origin = await settings();
  showServer();
  siteInput.value = new URLSearchParams(location.hash.slice(1)).get('site') ?? '';
  if (siteInput.value) {
    listInput.value =
      origin +
      ((await storage.get(pinPrefix(origin) + siteInput.value))[
        pinPrefix(origin) + siteInput.value
      ] ?? '');
    if (listInput.value === origin) listInput.value = '';
    listInput.focus();
  }
  await renderPins();
  await showPreviousDraft();
} catch (error) {
  status(error.message, true);
}
