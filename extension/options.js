import {
  defaultDestination,
  entrypointKey,
  inboxPath,
  pinPrefix,
  savedDestination,
  serverOrigin,
  settings,
  sitePattern,
  storage,
  supportsSidePanel,
  todoerPath,
} from './client.js';

const $ = (id) => document.getElementById(id);
const originInput = $('origin');
const siteInput = $('site');
const pinInput = $('pin-url');
const entrypoint = $('entrypoint');
const panelMode = $('panel-mode');
$('side-panel-option').disabled = !supportsSidePanel;
let origin;

function status(message, error = false) {
  $('status').textContent = message;
  $('status').classList.toggle('error', error);
}

async function renderPins() {
  const prefix = pinPrefix(origin);
  const pins = Object.entries(await storage.get(null))
    .filter(([key]) => key.startsWith(prefix))
    .sort(([a], [b]) => a.localeCompare(b));
  $('pins').replaceChildren();
  for (const [key, path] of pins) {
    const destination = savedDestination(origin, path);
    if (!destination) continue;
    const host = key.slice(prefix.length);
    const item = document.createElement('li');
    const site = document.createElement('span');
    site.textContent = host;
    const link = document.createElement('a');
    link.href = destination.href;
    link.textContent = path;
    link.target = '_blank';
    link.rel = 'noopener';
    link.setAttribute('aria-label', `Open Todoer link pinned to ${host}`);
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
    item.append(site, link, remove);
    $('pins').append(item);
  }
}

async function showServer() {
  originInput.value = origin;
  entrypoint.placeholder = origin + inboxPath;
  pinInput.placeholder = origin + '/space/…';
  const key = entrypointKey(origin);
  const saved = await storage.get([key, 'panelMode']);
  entrypoint.value = savedDestination(origin, saved[key])?.href ?? '';
  panelMode.value =
    supportsSidePanel && saved.panelMode === 'side-panel' ? 'side-panel' : 'popup';
  $('open-todoer').href = await defaultDestination(origin);
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
    await showServer();
    await renderPins();
    status('Todoer server saved.');
  } catch (error) {
    status(error.message, true);
  }
});

$('opening-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  try {
    const path = entrypoint.value.trim() ? todoerPath(entrypoint.value, origin) : null;
    await storage.set({ [entrypointKey(origin)]: path, panelMode: panelMode.value });
    $('open-todoer').href = await defaultDestination(origin);
    status('Opening preferences saved.');
  } catch (error) {
    status(error.message, true);
  }
});

$('pin-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  try {
    const host = sitePattern(siteInput.value);
    await storage.set({ [pinPrefix(origin) + host]: todoerPath(pinInput.value, origin) });
    await renderPins();
    status(`Saved the pin for ${host}.`);
  } catch (error) {
    status(error.message, true);
  }
});

try {
  origin = await settings();
  await showServer();
  siteInput.value = new URLSearchParams(location.hash.slice(1)).get('site') ?? '';
  if (siteInput.value) {
    const key = pinPrefix(origin) + siteInput.value;
    pinInput.value = savedDestination(origin, (await storage.get(key))[key])?.href ?? '';
    pinInput.focus();
  }
  await renderPins();
  await showPreviousDraft();
} catch (error) {
  status(error.message, true);
}
