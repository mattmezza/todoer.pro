import {
  enqueueCapture,
  replayCaptures,
  listPending,
  clearAccount,
  discardCapture,
} from './offline.js';
import { setupPwa, clearPwa } from './pwa.js';
const $ = (s) => document.querySelector(s);
const csrf = $('meta[name="csrf-token"]').content;
const list = document.body.dataset.list;
const account = $('meta[name="account-id"]').content;
setupPwa(account);
const dialog = $('#task-editor');
let editor = null;
let cursor = 0;
let busy = false;
let pendingRefresh = false;
const draftKey = `todoer:dev:draft:${list}`;
const input = $('#capture-title');
input.value = sessionStorage.getItem(draftKey) || '';
input.addEventListener('input', () => sessionStorage.setItem(draftKey, input.value));
function notice(text, error = false) {
  $('#notice').textContent = text;
  $('#notice').classList.toggle('error', error);
}
async function api(path, method = 'GET', body, extra = {}) {
  const response = await fetch(`/api/v1${path}`, {
    signal: AbortSignal.timeout?.(15000),
    method,
    headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': csrf, ...extra },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (response.status === 401) {
    notice('Your session ended. Copy any draft before signing in again.', true);
    throw new Error('Authentication required');
  }
  let data;
  try {
    data = await response.json();
  } catch {
    throw new Error('Unexpected server response. Your draft is safe.');
  }
  if (!response.ok) {
    const e = new Error(data.error?.message || 'Request failed');
    e.status = response.status;
    throw e;
  }
  return data;
}
function draftActive() {
  return (
    dialog.open ||
    input.value.length > 0 ||
    busy ||
    Boolean(window.getSelection()?.toString())
  );
}
function refresh(force = false) {
  if (!force && draftActive()) {
    pendingRefresh = true;
    $('#refresh-notice').hidden = false;
    return;
  }
  pendingRefresh = false;
  $('#refresh-notice').hidden = true;
  window.htmx.trigger('#task-list', 'tasks-refresh');
}
$('#refresh-now').onclick = () => {
  if (dialog.open) notice('Finish or close the task editor first.');
  else refresh(true);
};
document.body.addEventListener('htmx:afterSwap', (e) => {
  if (e.target.id === 'task-list')
    $('#task-count').textContent = $$('#task-list .task').length;
});
const $$ = (s) => [...document.querySelectorAll(s)];
document.body.addEventListener('htmx:responseError', () =>
  notice('Could not refresh tasks. Try again.', true),
);
$('#show-completed').onchange = (e) =>
  $('#task-list').classList.toggle('hide-completed', !e.target.checked);
$('#nav-toggle').onclick = () => {
  const open = $('#navigation').classList.toggle('open');
  $('#nav-toggle').setAttribute('aria-expanded', String(open));
};
$('#quick-add').onclick = () => input.focus();
document.addEventListener('keydown', (e) => {
  if (
    e.key === 'n' &&
    !e.ctrlKey &&
    !e.metaKey &&
    !e.altKey &&
    !e.target.closest('input,textarea,[contenteditable="true"]') &&
    !dialog.open
  ) {
    e.preventDefault();
    input.focus();
  }
});
let replaying = false;
async function showPending() {
  const records = await listPending(account);
  const container = $('#pending-captures');
  container.replaceChildren();
  for (const record of records) {
    const row = document.createElement('div');
    row.className = 'pending-row';
    const text = document.createElement('span');
    text.textContent = `${record.payload.tasks[0].title} · ${record.status === 'needs_attention' ? 'Needs attention' : 'Pending sync'}`;
    const discard = document.createElement('button');
    discard.textContent = 'Discard';
    discard.onclick = async () => {
      if (confirm('Discard this unsynced capture?')) {
        await discardCapture(account, record.key);
        showPending();
      }
    };
    row.append(text, discard);
    container.append(row);
  }
  $('#retry-captures').hidden = !records.length;
}
async function replay() {
  if (replaying) return;
  replaying = true;
  try {
    const result = await replayCaptures(account, (record) =>
      api('/capture', 'POST', record.payload, { 'Idempotency-Key': record.key }),
    );
    if (result.sent) {
      notice(`${result.sent} capture(s) synced.`);
      refresh();
    }
    if (result.errors.length && navigator.onLine) notice(result.errors[0].message, true);
    await showPending();
  } catch (err) {
    notice(`Local capture storage is unavailable: ${err.message}`, true);
  } finally {
    replaying = false;
  }
}
$('#capture-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  const title = input.value.trim();
  if (!title || busy) return;
  busy = true;
  const button = e.target.querySelector('button');
  button.disabled = true;
  try {
    await enqueueCapture(account, list, title);
    if (input.value.trim() === title) {
      input.value = '';
      sessionStorage.removeItem(draftKey);
    }
    notice('Capture saved on this device.');
    await showPending();
    if (navigator.onLine) await replay();
  } catch (err) {
    notice(
      `Could not save capture: ${err.message}. Keep this draft and try again.`,
      true,
    );
  } finally {
    busy = false;
    button.disabled = false;
    input.focus();
  }
});
$('#retry-captures').onclick = replay;
window.addEventListener('online', replay);
setInterval(() => {
  if (navigator.onLine) replay();
}, 15000);
replay();
$('#task-list').addEventListener('click', async (e) => {
  const row = e.target.closest('.task');
  if (!row) return;
  if (e.target.closest('.completion')) {
    const button = e.target.closest('.completion');
    if (button.disabled) return;
    const complete = row.dataset.complete !== 'true';
    button.disabled = true;
    row.classList.toggle('is-complete', complete);
    button.setAttribute('aria-pressed', String(complete));
    const previous = button.innerHTML;
    button.textContent = complete ? '✓' : '';
    try {
      const task = await api(`/tasks/${row.dataset.id}`, 'PATCH', {
        revision: Number(row.dataset.revision),
        complete,
      });
      row.dataset.revision = task.revision;
      row.dataset.complete = task.complete;
      button.setAttribute(
        'aria-label',
        `${task.complete ? 'Reopen' : 'Complete'} ${task.title}`,
      );
      notice(task.complete ? 'Task completed.' : 'Task reopened.');
    } catch (err) {
      row.classList.toggle('is-complete', !complete);
      button.setAttribute('aria-pressed', String(!complete));
      button.innerHTML = previous;
      notice(`${err.message} Your action was rolled back.`, true);
      if (err.status === 409) refresh();
    } finally {
      button.disabled = false;
    }
  } else if (e.target.closest('[data-action="edit"]')) {
    await openTask(row.dataset.id);
  }
});
function saveEditorDraft() {
  if (editor)
    sessionStorage.setItem(
      `todoer:dev:edit:${editor.id}`,
      JSON.stringify({
        title: $('#edit-title').value,
        description: $('#edit-description').value,
        revision: editor.revision,
      }),
    );
}
$('#edit-title').addEventListener('input', saveEditorDraft);
$('#edit-description').addEventListener('input', saveEditorDraft);
function closeEditor() {
  dialog.close();
  if (location.search.includes('task=')) {
    if (history.state?.todoerPanel) history.back();
    else history.replaceState(null, '', location.pathname);
  }
  if (pendingRefresh) refresh();
}
$('#close-editor').onclick = closeEditor;
$('#cancel-edit').onclick = closeEditor;
dialog.addEventListener('cancel', (e) => {
  e.preventDefault();
  closeEditor();
});
window.addEventListener('popstate', () => {
  const id = new URLSearchParams(location.search).get('task');
  if (id) openTask(id, false);
  else if (dialog.open) dialog.close();
});
$('#edit-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  if (!editor) return;
  const button = e.target.querySelector('button[type="submit"]');
  button.disabled = true;
  try {
    await api(`/tasks/${editor.id}`, 'PATCH', {
      revision: editor.revision,
      title: $('#edit-title').value,
      description: $('#edit-description').value,
    });
    sessionStorage.removeItem(`todoer:dev:edit:${editor.id}`);
    closeEditor();
    notice('Changes saved.');
    refresh(true);
  } catch (err) {
    $('#edit-error').textContent =
      `${err.message}. Your draft is preserved. Close to review the latest task; copy your draft before discarding it.`;
  } finally {
    button.disabled = false;
  }
});
$('#logout').onclick = async () => {
  const pending = await listPending(account);
  if (
    pending.length &&
    !confirm(
      `${pending.length} capture(s) have not synced. Signing out clears their local copies. Sync first or confirm to discard and sign out.`,
    )
  )
    return;
  try {
    const response = await fetch('/logout', {
      method: 'POST',
      headers: { 'X-CSRF-Token': csrf },
    });
    if (!response.ok) throw new Error('Sign-out failed');
    await clearAccount(account);
    await clearPwa(account);
    for (const key of Object.keys(sessionStorage))
      if (key.startsWith('todoer:')) sessionStorage.removeItem(key);
    location.href = '/signin';
  } catch {
    notice(
      'Reconnect to sign out and revoke this session. Local captures are preserved.',
      true,
    );
  }
};
if ('EventSource' in window) {
  const events = new EventSource(`/api/v1/events?after=${cursor}`);
  events.addEventListener('changes', (e) => {
    const data = JSON.parse(e.data);
    if (data.resync || data.cursor > cursor) {
      cursor = data.cursor || cursor;
      refresh();
    }
    $('#connection').textContent = 'Up to date';
  });
  events.addEventListener('session-expired', () => {
    events.close();
    $('#connection').textContent = 'Session ended';
    notice('Your session ended. Preserve any draft and sign in again.', true);
  });
  events.onerror = () => {
    $('#connection').textContent = navigator.onLine ? 'Reconnecting' : 'Offline';
  };
} else {
  const poll = async () => {
    try {
      const changes = await api(`/changes?after=${cursor}`);
      if (changes.length) {
        cursor = Math.max(cursor, ...changes.map((c) => c.seq));
        refresh();
      }
      $('#connection').textContent = 'Up to date';
    } catch {
      $('#connection').textContent = 'Reconnecting';
    }
    setTimeout(poll, 3000);
  };
  poll();
}
// Direct URLs work even when the task is not currently in the rendered rows.
const taskId = new URLSearchParams(location.search).get('task');
if (taskId) openTask(taskId, false);
$('#trash-task').onclick = async () => {
  if (!editor) return;
  if (
    !confirm(
      'Move this task and its entire checklist to trash? Unsaved edits stay in your draft.',
    )
  )
    return;
  try {
    await api(`/tasks/${editor.id}/trash`, 'POST', { revision: editor.revision });
    closeEditor();
    notice('Task moved to trash. Restore it from Trash.');
    refresh(true);
  } catch (err) {
    $('#edit-error').textContent = err.message;
  }
};

async function openTask(id, push = true) {
  try {
    const tasks = await api(`/tasks/${id}/subtree`);
    editor = tasks.find((t) => t.id === id || t.reference === id) || tasks[0];
    if (!editor) throw new Error('Task not found');
    $('#edit-title').value = editor.title;
    $('#edit-description').value = editor.description;
    $('#editor-reference').textContent = `%${editor.reference}`;
    $('#edit-error').textContent = '';
    const saved = sessionStorage.getItem(`todoer:dev:edit:${editor.id}`);
    if (saved) {
      const draft = JSON.parse(saved);
      $('#edit-title').value = draft.title;
      $('#edit-description').value = draft.description;
      editor.revision = draft.revision;
      $('#edit-error').textContent =
        'Restored your unsaved draft. Saving checks its original revision.';
    }
    renderChildren(tasks);
    $('#move-destination').value = editor.list_id;
    if (!dialog.open) dialog.showModal();
    if (push)
      history.pushState({ task: editor.id, todoerPanel: true }, '', `?task=${editor.id}`);
    $('#edit-title').focus();
  } catch (err) {
    notice(err.message, true);
  }
}
function renderChildren(tasks) {
  const children = tasks.filter((t) => t.parent_id === editor.id);
  // Progress counts leaves in this subtree, once each. A parent container is not
  // counted again when its descendants also appear.
  const parents = new Set(tasks.map((t) => t.parent_id).filter(Boolean));
  const leaves = tasks.filter((t) => t.id !== editor.id && !parents.has(t.id));
  $('#subtask-progress').textContent = leaves.length
    ? `${leaves.filter((t) => t.complete).length}/${leaves.length}`
    : '';
  $('#subtask-list').replaceChildren();
  for (const child of children) {
    const row = document.createElement('div');
    row.className = 'subtask-row';
    const complete = document.createElement('button');
    complete.type = 'button';
    complete.className = 'child-complete';
    complete.textContent = child.complete ? '✓' : '○';
    complete.setAttribute(
      'aria-label',
      `${child.complete ? 'Reopen' : 'Complete'} ${child.title}`,
    );
    complete.onclick = async () => {
      complete.disabled = true;
      try {
        await api(`/tasks/${child.id}`, 'PATCH', {
          revision: child.revision,
          complete: !child.complete,
        });
        renderChildren(await api(`/tasks/${editor.id}/subtree`));
        pendingRefresh = true;
      } catch (err) {
        $('#edit-error').textContent = err.message;
      } finally {
        complete.disabled = false;
      }
    };
    const title = document.createElement('button');
    title.type = 'button';
    title.className = 'subtask-open';
    title.textContent = child.title;
    title.onclick = () => openTask(child.id);
    const deeper = document.createElement('span');
    deeper.textContent = tasks.some((t) => t.parent_id === child.id)
      ? 'Open checklist →'
      : '→';
    deeper.className = 'muted';
    row.append(complete, title, deeper);
    $('#subtask-list').append(row);
  }
}
let childCapture = null;
$('#add-subtask').onclick = async () => {
  const title = $('#subtask-title').value.trim();
  if (!editor || !title) return;
  const parent = editor.id;
  if (!childCapture || childCapture.title !== title || childCapture.parent !== parent)
    childCapture = { title, parent, key: crypto.randomUUID() };
  const button = $('#add-subtask');
  button.disabled = true;
  try {
    await api(
      '/capture',
      'POST',
      { list_id: editor.list_id, tasks: [{ title, parent_id: parent, description: '' }] },
      { 'Idempotency-Key': childCapture.key },
    );
    $('#subtask-title').value = '';
    childCapture = null;
    renderChildren(await api(`/tasks/${parent}/subtree`));
    pendingRefresh = true;
  } catch (err) {
    $('#edit-error').textContent = err.message;
  } finally {
    button.disabled = false;
  }
};
$('#subtask-title').addEventListener('keydown', (e) => {
  if (e.key === 'Enter') {
    e.preventDefault();
    $('#add-subtask').click();
  }
});

$('#move-task').onclick = async () => {
  if (!editor) return;
  if (
    $('#edit-title').value !== editor.title ||
    $('#edit-description').value !== editor.description
  ) {
    $('#edit-error').textContent = 'Save your edits before moving this task.';
    return;
  }
  const destination = $('#move-destination').value;
  if (destination === editor.list_id) {
    $('#edit-error').textContent = 'Choose a different destination list.';
    return;
  }
  try {
    const preview = await api(
      `/tasks/${editor.id}/move-preview?destination=${encodeURIComponent(destination)}`,
    );
    let acknowledge_access_change = false;
    if (preview.audience_broadens) {
      acknowledge_access_change = confirm(
        `This move gives ${preview.added_members} additional member(s) access to this task and its entire checklist. Allow this access change?`,
      );
      if (!acknowledge_access_change) return;
    }
    await api(`/tasks/${editor.id}/move`, 'POST', {
      destination,
      revision: editor.revision,
      acknowledge_access_change,
    });
    closeEditor();
    notice('Task and checklist moved. IDs and references are preserved.');
    refresh(true);
  } catch (err) {
    $('#edit-error').textContent = err.message;
  }
};
