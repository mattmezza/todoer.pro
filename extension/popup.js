import {
  ApiError,
  apiPath,
  browserApi,
  getLists,
  preferenceKey,
  request,
  siteHost,
  storage,
} from './client.js';

const $ = (id) => document.getElementById(id);
const select = $('lists');
const pin = $('pin');
const title = $('title');
const tasks = $('tasks');
let connection;
let lists;
let hostname;
let pinKey;
let pinnedList;
let preferredList;
let pendingKey;
let pending;
let listRequest = 0;
let submitting = false;

$('settings').addEventListener('click', () => browserApi.runtime.openOptionsPage());

function status(message, error = false) {
  $('status').textContent = message;
  $('status').classList.toggle('error', error);
}

function showPin() {
  pin.hidden = !hostname;
  const isPinned = pinnedList === select.value;
  pin.textContent = isPinned ? `Unpin from ${hostname}` : `Pin to ${hostname}`;
  pin.dataset.pinned = String(isPinned);
}

function showCapture() {
  const isAvailable = !pending || lists.some((list) => list.id === pending.listId);
  // A pending capture retains its original destination even on a different website.
  if (pending && isAvailable) select.value = pending.listId;
  select.disabled = submitting || Boolean(pending);
  pin.disabled = submitting || Boolean(pending);
  title.readOnly = submitting || Boolean(pending);
  $('add').disabled = submitting || !isAvailable;
  $('add').textContent = pending ? 'Retry add' : 'Add';
  $('discard').hidden = !pending;
  $('discard').disabled = submitting;
  if (pending) title.value = pending.title;
  showPin();
}

function taskRow(task) {
  const row = document.createElement('li');
  const complete = document.createElement('button');
  complete.type = 'button';
  complete.className = 'complete';
  complete.setAttribute('aria-label', `Complete ${task.title}`);
  const link = document.createElement('a');
  link.href = connection.origin + apiPath('tasks', task.public_reference ?? task.id);
  link.target = '_blank';
  link.rel = 'noopener noreferrer';
  link.textContent = task.title;
  row.append(complete, link);
  complete.addEventListener('click', async () => {
    complete.disabled = true;
    try {
      await request(connection, apiPath('tasks', task.id), {
        method: 'PATCH',
        body: { revision: task.revision, complete: true },
      });
      if (select.value === task.list_id) {
        status('Task completed.');
        await loadTasks();
      }
    } catch (error) {
      if (select.value === task.list_id) {
        status(error.message, true);
        if (error.status === 409) await loadTasks();
      }
    } finally {
      complete.disabled = false;
    }
  });
  return row;
}

async function loadTasks() {
  const current = ++listRequest;
  const list = lists.find(({ id }) => id === select.value);
  $('open-list').href = connection.origin + apiPath('lists', list.reference ?? list.id);
  tasks.replaceChildren();
  $('list-status').textContent = 'Loading tasks…';
  try {
    const loaded = await request(connection, `${apiPath('lists', list.id)}/tasks`);
    if (current !== listRequest) return;
    const open = loaded.filter((task) => !task.complete);
    tasks.replaceChildren(...open.map(taskRow));
    $('list-status').textContent = open.length ? '' : 'No open tasks.';
  } catch (error) {
    if (current === listRequest) $('list-status').textContent = error.message;
  }
}

select.addEventListener('change', () => {
  preferredList = select.value;
  status('');
  showPin();
  loadTasks();
});

pin.addEventListener('click', async () => {
  const id = select.value;
  pin.disabled = true;
  try {
    if (pinnedList === id) {
      await storage.remove(pinKey);
      pinnedList = undefined;
      status(`Unpinned ${hostname}.`);
    } else {
      await storage.set({ [pinKey]: id });
      pinnedList = id;
      status(`Pinned to ${hostname}.`);
    }
  } catch (error) {
    status(error.message, true);
  } finally {
    showCapture();
  }
});

$('capture').addEventListener('submit', async (event) => {
  event.preventDefault();
  if (submitting) return;
  const value = title.value.trim();
  if (!value || [...value].length > 500) {
    status('Enter a task title of 1–500 characters.', true);
    title.focus();
    return;
  }

  submitting = true;
  showCapture();
  status('Adding task…');
  try {
    await navigator.locks.request(pendingKey, async () => {
      const saved = await storage.get(['connection', pendingKey]);
      if (
        saved.connection?.token !== connection.token ||
        saved.connection?.id !== connection.id
      ) {
        throw new Error('The connection changed. Reopen Todoer before adding a task.');
      }
      if (saved[pendingKey] && saved[pendingKey].key !== pending?.key) {
        pending = saved[pendingKey];
        throw new Error(
          'A task is already pending. Retry it or discard it before adding another.',
        );
      }
      const capture = pending ?? {
        title: value,
        listId: select.value,
        key: crypto.randomUUID(),
      };
      await storage.set({ [pendingKey]: capture });
      pending = capture;
      try {
        await request(connection, '/capture', {
          method: 'POST',
          key: capture.key,
          body: { list_id: capture.listId, tasks: [{ title: capture.title }] },
        });
      } catch (error) {
        // A definite rejection can be corrected. An uncertain outcome keeps its original key.
        if (
          error instanceof ApiError &&
          error.status >= 400 &&
          error.status < 500 &&
          error.status !== 408
        ) {
          await storage.remove(pendingKey);
          pending = undefined;
        }
        throw error;
      }
      await storage.remove(pendingKey);
      pending = undefined;
      title.value = '';
      select.value = preferredList;
    });
    status('Task added.');
    await loadTasks();
  } catch (error) {
    status(
      pending ? `${error.message} Retry to check the same task.` : error.message,
      true,
    );
  } finally {
    submitting = false;
    showCapture();
    if (!pending) title.focus();
  }
});

$('discard').addEventListener('click', async () => {
  if (submitting || !pending) return;
  submitting = true;
  showCapture();
  try {
    await navigator.locks.request(pendingKey, async () => {
      const saved = (await storage.get(pendingKey))[pendingKey];
      if (saved?.key === pending.key) await storage.remove(pendingKey);
    });
    pending = undefined;
    title.value = '';
    select.value = preferredList;
    status('Pending task cleared. Check the list for any task already saved.');
    await loadTasks();
  } catch (error) {
    status(error.message, true);
  } finally {
    submitting = false;
    showCapture();
  }
});

async function start() {
  try {
    // Query the active tab while the action's temporary activeTab permission is in effect.
    const [tab] = await browserApi.tabs.query({ active: true, currentWindow: true });
    hostname = siteHost(tab?.url);
    ({ connection } = await storage.get('connection'));
    if (!connection) {
      status('Open Settings to connect your Todoer account.');
      return;
    }
    pinKey = preferenceKey(connection, `pin:${hostname}`);
    pendingKey = preferenceKey(connection, 'pending');
    const [available, preferences] = await Promise.all([
      getLists(connection),
      storage.get([pinKey, pendingKey]),
    ]);
    lists = available;
    pending = preferences[pendingKey];
    if (!lists.length) {
      status('This credential cannot see any lists. Open Settings to change its access.');
      return;
    }

    pinnedList = hostname ? preferences[pinKey] : undefined;
    const selected =
      lists.find((list) => list.id === pinnedList) ??
      lists.find((list) => list.title.toLowerCase() === 'inbox') ??
      lists[0];
    for (const list of lists) {
      const label = lists.some(
        (other) => other.id !== list.id && other.title === list.title,
      )
        ? `${list.title} · #${list.reference ?? list.id}`
        : list.title;
      select.add(new Option(label, list.id));
    }
    preferredList = selected.id;
    select.value = preferredList;
    showCapture();
    $('workspace').hidden = false;
    if (pending) {
      status(
        lists.some((list) => list.id === pending.listId)
          ? 'A task is pending. Retry to check whether it was saved.'
          : 'The pending task’s list is no longer accessible.',
        true,
      );
    } else {
      status('');
    }
    await loadTasks();
  } catch (error) {
    status(error.message, true);
  }
}

start();
