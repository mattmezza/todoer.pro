import {
  browserApi,
  cloudOrigin,
  getLists,
  preferenceKey,
  serverOrigin,
  storage,
} from './client.js';
import { revokeSignIn, signIn } from './sign-in.js';

const $ = (id) => document.getElementById(id);
const form = $('connection-form');
const originInput = $('origin');
const tokenInput = $('token');
const disconnect = $('disconnect');
let connection;
let lists = [];

function status(message, error = false) {
  $('status').textContent = message;
  $('status').classList.toggle('error', error);
}

function updateCredentialLink() {
  try {
    $('credentials').href = `${serverOrigin(originInput.value)}/settings/ai/tools`;
  } catch {
    $('credentials').removeAttribute('href');
  }
}

async function renderPins() {
  const list = $('pin-list');
  list.replaceChildren();
  $('pins').hidden = !connection;
  if (!connection) return;
  const prefix = preferenceKey(connection, 'pin:');
  const pins = Object.entries(await storage.get(null))
    .filter(([key]) => key.startsWith(prefix))
    .sort(([a], [b]) => a.localeCompare(b));
  for (const [key, id] of pins) {
    const host = key.slice(prefix.length);
    const item = document.createElement('li');
    const label = document.createElement('span');
    const title = lists.find((entry) => entry.id === id)?.title ?? 'Saved list';
    label.textContent = `${host} · ${title}`;
    const remove = document.createElement('button');
    remove.type = 'button';
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
    item.append(label, remove);
    list.append(item);
  }
  if (!pins.length) {
    const empty = document.createElement('li');
    empty.textContent = 'No websites pinned yet.';
    list.append(empty);
  }
}

function showConnection() {
  originInput.value = connection?.origin ?? cloudOrigin;
  tokenInput.value = '';
  disconnect.hidden = !connection;
  $('connect').textContent = connection ? 'Sign in again' : 'Sign in with Todoer';
  updateCredentialLink();
}

async function clearPreferences(previous) {
  const prefix = preferenceKey(previous, '');
  const keys = Object.keys(await storage.get(null)).filter((key) =>
    key.startsWith(prefix),
  );
  await storage.remove(keys);
}

function busy(value) {
  for (const control of [
    originInput,
    tokenInput,
    $('connect'),
    $('manual-connect'),
    disconnect,
  ]) {
    control.disabled = value;
  }
}

async function allowServer(origin) {
  if (origin === cloudOrigin) return;
  // Request optional host access directly from the browser gesture.
  const url = new URL(origin);
  const origins = [`${url.protocol}//${url.hostname}/*`];
  if (!(await browserApi.permissions.request({ origins }))) {
    throw new Error('Allow browser access to this Todoer server to connect.');
  }
}

originInput.addEventListener('input', updateCredentialLink);
form.addEventListener('submit', async (event) => {
  event.preventDefault();
  try {
    const origin = serverOrigin(originInput.value);
    const manual =
      event.submitter === $('manual-connect') || document.activeElement === tokenInput;
    const pasted = tokenInput.value.trim();
    if (manual && !pasted) throw new Error('Enter a REST credential.');
    busy(true);
    await allowServer(origin);
    status(
      manual
        ? 'Checking connection…'
        : 'Approve the extension in the Todoer sign-in window…',
    );
    const token = manual ? pasted : await signIn(origin);
    const previous = connection;
    const sameServer = previous?.origin === origin;
    const sameCredential = sameServer && previous.token === token;
    const next = {
      origin,
      token,
      id: sameServer ? previous.id : crypto.randomUUID(),
      managed: !manual || (sameCredential && previous.managed === true),
    };
    try {
      const available = await getLists(next);
      // Pending captures belong to the credential that started them.
      if (sameServer && !sameCredential) {
        await storage.remove(preferenceKey(previous, 'pending'));
      }
      await storage.set({ connection: next });
      connection = next;
      lists = available;
    } catch (error) {
      if (!sameCredential) await revokeSignIn(next).catch(() => {});
      throw error;
    }
    let oldAccess = '';
    if (previous && !sameCredential) {
      try {
        await revokeSignIn(previous);
      } catch {
        oldAccess = ' Revoke the previous credential in Todoer’s Connected tools.';
      }
    }
    if (previous && !sameServer) await clearPreferences(previous);
    showConnection();
    await renderPins();
    status(
      (lists.length
        ? 'Connected. Open Todoer from the browser toolbar.'
        : 'Connected. This credential cannot see any lists.') + oldAccess,
    );
  } catch (error) {
    status(error.message, true);
  } finally {
    busy(false);
  }
});

disconnect.addEventListener('click', async () => {
  busy(true);
  try {
    const previous = connection;
    await clearPreferences(previous);
    await storage.remove('connection');
    connection = undefined;
    lists = [];
    showConnection();
    await renderPins();
    status('Disconnected.');
    if (!previous.managed) {
      status('Disconnected. You can revoke the credential in Todoer’s Connected tools.');
    } else {
      try {
        await revokeSignIn(previous);
      } catch {
        status('Disconnected. Revoke the credential in Todoer’s Connected tools.');
      }
    }
  } catch (error) {
    status(error.message, true);
  } finally {
    busy(false);
  }
});

async function start() {
  busy(true);
  try {
    ({ connection } = await storage.get('connection'));
    showConnection();
    if (connection) {
      try {
        lists = await getLists(connection);
        status(`Connected to ${connection.origin}.`);
      } catch (error) {
        status(error.message, true);
      }
    }
    await renderPins();
  } catch (error) {
    status(error.message, true);
  } finally {
    busy(false);
  }
}

start();
