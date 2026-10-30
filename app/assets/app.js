import { reconcileTasks } from './task-list-refresh.js';
import { loadingDialog } from './loading.js';
import { request as fetch } from './request.js';
import { stowInfo } from './quiet-info.js';
import { confirmAction } from './confirm.js';
import { mountCaptures } from './capture-ui.js';
import './site.js';
import { mountListUi } from './list-ui.js';
import { mountDiscussion } from './discussion.js';
import { mountScheduling } from './scheduling.js';
import { mountTaskMetadata } from './task-metadata.js';
import { mountTaskOrdering } from './task-order.js';
import { replayCaptures, listPending, clearAccount, discardCapture } from './offline.js';
import { clearPwa } from './pwa.js';
const $ = (s) => document.querySelector(s);
const csrf = $('meta[name="csrf-token"]').content;
const list = document.body.dataset.list;
const account = $('meta[name="account-id"]').content;
const canWrite = document.body.dataset.canWrite === 'true';
function restoreMainCapture() {
  const slot = document.querySelector('[data-main-capture-slot]');
  if (slot && !document.querySelector('#capture-form'))
    slot.append(document.querySelector('#main-capture-template').content.cloneNode(true));
}
restoreMainCapture();
document.body.addEventListener('htmx:afterSwap', restoreMainCapture);
const dialog = $('#task-editor');
dialog.addEventListener('close', () => {
  discussionCleanup?.();
  discussionCleanup = null;
  schedulingCleanup?.();
  schedulingCleanup = null;
  metadataCleanup?.();
  metadataCleanup = null;
});
let editor = null;
let discussionCleanup = null;
let schedulingCleanup = null;
let metadataCleanup = null;
let editorBusy = false;
let editorOpening = false;
let taskPointerDown = false;
// Do not replace a row between pointerdown and click. Firefox otherwise drops
// a click when a live update removes its original event target.
document.addEventListener(
  'pointerdown',
  (event) => {
    taskPointerDown = Boolean(event.target.closest?.('#task-list'));
  },
  true,
);
for (const type of ['pointerup', 'pointercancel'])
  document.addEventListener(
    type,
    () => {
      setTimeout(() => {
        taskPointerDown = false;
        if (pendingRefresh && !draftActive()) refresh();
      }, 0);
    },
    true,
  );
let editorOpenSequence = 0;
let cursor = Number(document.body.dataset.cursor) || 0;
let busy = false;
let pendingRefresh = false;
const inlineEditors = new Map();
let restoreTaskFocus = null;
const draftKey = `todoer:${account}:draft:${list}`;
const input = $('#capture-title');
input.value = sessionStorage.getItem(draftKey) || '';

function notice(text, error = false) {
  if (!error && stowInfo(text, 'list')) {
    $('#notice').textContent = '';
    return;
  }
  $('#notice').textContent = text;
  $('#notice').classList.toggle('error', error);
}
async function api(path, method = 'GET', body, extra = {}) {
  if (method !== 'GET' && !canWrite)
    throw new Error('You have read-only access to this list.');
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
    taskPointerDown ||
    editorOpening ||
    inlineEditors.size > 0 ||
    Boolean($('#task-list .task-actions[open]')) ||
    dialog.open ||
    [
      ...document.querySelectorAll(
        '.inline-capture:not([hidden]) input[name=title],.inline-capture:not([hidden]) textarea',
      ),
    ].some((field) => field.value.length > 0) ||
    busy ||
    Boolean(window.getSelection()?.toString())
  );
}
function refresh(force = false) {
  if (inlineEditors.size || dialog.open || (!force && draftActive())) {
    pendingRefresh = true;
    $('#refresh-notice').hidden = false;
    return;
  }
  pendingRefresh = false;
  $('#refresh-notice').hidden = true;
  const active = document.activeElement;
  const focused = active.closest?.('.task');
  restoreTaskFocus = focused
    ? {
        id: focused.dataset.id,
        selector: active.matches('.completion')
          ? '.completion'
          : active.matches('.task-menu')
            ? '.task-menu'
            : '.task-title',
      }
    : null;
  window.htmx.trigger('#task-list', 'tasks-refresh');
}
$('#refresh-now').onclick = () => {
  if (dialog.open || inlineEditors.size) notice('Finish or close the task editor first.');
  else refresh(true);
};
document.body.addEventListener('htmx:beforeSwap', (e) => {
  if (e.detail.target?.id === 'task-list' && draftActive()) {
    e.detail.shouldSwap = false;
    pendingRefresh = true;
    $('#refresh-notice').hidden = false;
  } else if (e.detail.target?.id === 'task-list' && e.detail.shouldSwap) {
    const incoming = document.createElement('div');
    incoming.innerHTML = e.detail.serverResponse;
    reconcileTasks(e.detail.target, incoming);
    e.detail.shouldSwap = false;
    e.detail.target.dispatchEvent(
      new CustomEvent('htmx:afterSwap', {
        bubbles: true,
        detail: { target: e.detail.target },
      }),
    );
    $('#task-count').textContent = document.querySelectorAll('#task-list .task').length;
  }
});
document.body.addEventListener('htmx:afterSwap', (e) => {
  if (e.target.id === 'task-list')
    $('#task-count').textContent = $$('#task-list .task').length;
  if (
    e.target.id === 'task-list' &&
    restoreTaskFocus &&
    (document.activeElement === document.body ||
      $('#task-list').contains(document.activeElement))
  ) {
    const row = $$('#task-list .task').find((r) => r.dataset.id === restoreTaskFocus.id);
    row?.querySelector(restoreTaskFocus.selector)?.focus({ preventScroll: true });
    restoreTaskFocus = null;
  }
});
const $$ = (s) => [...document.querySelectorAll(s)];
document.body.addEventListener('htmx:responseError', () =>
  notice('Could not refresh tasks. Try again.', true),
);
$('#show-completed').onchange = (e) =>
  $('#task-list').classList.toggle('hide-completed', !e.target.checked);

$('#quick-add').onclick = () => {
  $('#capture-form').hidden = false;
  $('#capture-title')?.focus();
};
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
    $('#capture-form').hidden = false;
    $('#capture-title')?.focus();
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
      if (await confirmAction('Discard this unsynced capture?')) {
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
mountCaptures({
  account,
  listId: list,
  canWrite,
  notice,
  onQueued: async () => {
    await showPending();
    if (navigator.onLine) await replay();
  },
  setBusy: (value) => {
    busy = value;
  },
  onSettled: () => {
    if (pendingRefresh) refresh();
  },
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
    button.innerHTML = complete
      ? '<svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#check"/></svg>'
      : '';
    try {
      const task = await api(`/tasks/${row.dataset.id}`, 'PATCH', {
        revision: Number(row.dataset.revision),
        complete,
      });
      row.dataset.revision = task.revision;
      row.dataset.complete = task.complete;
      const section = row.closest('.task-section');
      if (section) {
        const rows = [...section.querySelectorAll('.task')],
          parents = new Set(rows.map((r) => r.dataset.parentId).filter(Boolean)),
          leaves = rows.filter((r) => !parents.has(r.dataset.id)),
          completed = leaves.filter((r) => r.dataset.complete === 'true').length,
          progress = leaves.length ? Math.floor((completed * 100) / leaves.length) : 0;
        section
          .querySelector('.section-heading')
          ?.style.setProperty('--progress', `${progress}%`);
        const label = section.querySelector('.section-progress');
        if (label) {
          label.textContent = `${progress}%`;
          label.setAttribute(
            'aria-label',
            `${completed} of ${leaves.length} leaf tasks complete`,
          );
        }
      }
      button.setAttribute(
        'aria-label',
        `${task.complete ? 'Reopen' : 'Complete'} ${task.title}`,
      );
      notice('');
    } catch (err) {
      row.classList.toggle('is-complete', !complete);
      button.setAttribute('aria-pressed', String(!complete));
      button.innerHTML = previous;
      notice(
        `${err.message} The previous display has been restored; refresh to confirm the saved state.`,
        true,
      );
      if (err.status === 409) refresh();
    } finally {
      button.disabled = false;
    }
  } else if (e.target.closest('[data-action="complete-subtree"]')) {
    row.querySelector('details')?.removeAttribute('open');
    if (
      !canWrite ||
      !(await confirmAction('Complete this task and every unfinished sub-task?'))
    )
      return;
    const button = e.target.closest('button');
    button.disabled = true;
    try {
      await api(`/tasks/${row.dataset.id}`, 'PATCH', {
        revision: Number(row.dataset.revision),
        complete: true,
        complete_subtree: true,
      });
      notice('');
      await refresh(true);
    } catch (error) {
      notice(error.message, true);
    } finally {
      button.disabled = false;
    }
  } else if (e.target.closest('[data-action="rename"]')) {
    row.querySelector('details')?.removeAttribute('open');
    await renameInline(row);
  } else if (e.target.closest('[data-action="edit"]')) {
    row.querySelector('details')?.removeAttribute('open');
    await openTask(row.dataset.id);
  }
});
function saveEditorDraft() {
  if (editor)
    sessionStorage.setItem(
      `todoer:${account}:edit:${editor.id}`,
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
  if (editorBusy) return;
  editorOpenSequence++;
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
  else {
    editorOpenSequence++;
    if (dialog.open) dialog.close();
  }
});
function freezeEditor() {
  editorBusy = true;
  const controls = [...$('#edit-form').elements].map((el) => [el, el.disabled]);
  for (const [el] of controls) el.disabled = true;
  $('#edit-form').setAttribute('aria-busy', 'true');
  let released = false;
  return () => {
    if (released) return;
    released = true;
    for (const [el, disabled] of controls) el.disabled = disabled;
    editorBusy = false;
    $('#edit-form').removeAttribute('aria-busy');
  };
}
function settleEditorRoute() {
  const id = new URLSearchParams(location.search).get('task');
  if (id && (!dialog.open || editor?.id !== id)) openTask(id, false);
}
$('#edit-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  if (!editor || editorBusy) return;
  const task = { ...editor };
  const title = $('#edit-title').value;
  const description = $('#edit-description').value;
  const release = freezeEditor();
  let closed = false;
  try {
    await api(`/tasks/${task.id}`, 'PATCH', {
      revision: task.revision,
      title,
      description,
    });
    sessionStorage.removeItem(`todoer:${account}:edit:${task.id}`);
    release();
    if (dialog.open && editor?.id === task.id) {
      closeEditor();
      closed = true;
    }
    notice('');
    refresh(true);
  } catch (err) {
    $('#edit-error').textContent =
      `${err.message}. Your draft is preserved. Close to review the latest task; copy your draft before discarding it.`;
  } finally {
    release();
    if (!closed) settleEditorRoute();
  }
});

if ('EventSource' in window) {
  const events = new EventSource(
    `/api/v1/events?after=${cursor}&list_id=${encodeURIComponent(list)}`,
  );
  events.addEventListener('changes', (e) => {
    const data = JSON.parse(e.data);
    if (data.resync || (data.cursor > cursor && data.count > 0)) {
      discussionCleanup?.refresh?.();
      cursor = data.cursor || cursor;
      refresh();
    }
    cursor = Math.max(cursor, data.cursor || cursor);
    $('#connection').textContent = 'Up to date';
  });
  events.addEventListener('access-revoked', async () => {
    events.close();
    saveEditorDraft();
    await clearPwa(account);
    $('#task-list').replaceChildren();
    dialog.close();
    $('#edit-title').value = '';
    $('#edit-description').value = '';
    $('#subtask-list').replaceChildren();
    $('#editor-reference').textContent = '';
    $('#discussion-container').replaceChildren();
    $('#schedule-container').replaceChildren();
    $('#metadata-container').replaceChildren();
    document.querySelector('.list-description')?.remove();
    document.querySelector('.breadcrumbs').replaceChildren();
    editor = null;
    $('#capture-form').hidden = true;
    $('#quick-add').disabled = true;
    document.querySelector('nav[aria-label="Lists"]').replaceChildren();
    document.querySelector('h1').textContent = 'Access changed';
    document.title = 'Access changed · Todoer';
    $('#connection').textContent = 'Access removed';
    notice(
      'Your access to this list was removed. Unsaved drafts remain on this device under your account.',
      true,
    );
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
        if (changes.some((c) => c.task?.list_id === list || c.source_list_id === list))
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
  if (!editor || editorBusy) return;
  if (
    !(await confirmAction(
      'Move this task and all its sub-tasks to trash? Unsaved edits stay in your draft.',
    ))
  )
    return;
  const task = { ...editor };
  const release = freezeEditor();
  let closed = false;
  try {
    await api(`/tasks/${task.id}/trash`, 'POST', { revision: task.revision });
    release();
    if (dialog.open && editor?.id === task.id) {
      closeEditor();
      closed = true;
    }
    notice('');
    refresh(true);
  } catch (err) {
    $('#edit-error').textContent = err.message;
  } finally {
    release();
    if (!closed) settleEditorRoute();
  }
};

async function openTask(id, push = true) {
  if (editorBusy) return;
  const sequence = ++editorOpenSequence;
  editorOpening = true;
  const pending = dialog.open ? null : loadingDialog('Loading task', 'capture-omnibar');
  try {
    const tasks = await api(`/tasks/${id}/subtree`);
    if (sequence !== editorOpenSequence || (pending && !pending.dialog.open)) return;
    discussionCleanup?.();
    discussionCleanup = null;
    schedulingCleanup?.();
    schedulingCleanup = null;
    metadataCleanup?.();
    metadataCleanup = null;
    editor = tasks.find((t) => t.id === id || t.reference === id) || tasks[0];
    if (!editor) throw new Error('Task not found');
    $('#edit-title').value = editor.title;
    $('#edit-description').value = editor.description;
    $('#editor-reference').textContent =
      `${editor.parent_id ? '^' : '%'}${editor.reference}`;
    $('#edit-error').textContent = '';
    const saved = sessionStorage.getItem(`todoer:${account}:edit:${editor.id}`);
    if (saved) {
      const draft = JSON.parse(saved);
      $('#edit-title').value = draft.title;
      $('#edit-description').value = draft.description;
      editor.revision = draft.revision;
      $('#edit-error').textContent =
        'Restored your unsaved draft. Saving checks its original revision.';
    }
    for (const control of dialog.querySelectorAll('input,textarea'))
      control.readOnly = !canWrite;
    for (const selector of [
      '#add-subtask',
      '#move-task',
      '#trash-task',
      '#edit-form button[type="submit"]',
    ])
      $(selector).hidden = !canWrite;
    document.querySelector('.subtask-add').hidden = !canWrite;
    renderChildren(tasks);
    discussionCleanup = mountDiscussion({
      taskId: editor.id,
      csrf,
      container: $('#discussion-container'),
      accountId: account,
      onConflict: (err) => notice(err.message, true),
    });
    const scheduledTaskId = editor.id;
    schedulingCleanup = mountScheduling({
      taskId: scheduledTaskId,
      accountId: account,
      csrf,
      canWrite,
      container: $('#schedule-container'),
      getRevision: () =>
        editor?.id === scheduledTaskId && !editorBusy ? editor.revision : null,
      onSaved: (expected, revision) => {
        if (editor?.id !== scheduledTaskId || editor.revision !== expected) return false;
        editor.revision = revision;
        if (sessionStorage.getItem(`todoer:${account}:edit:${editor.id}`))
          saveEditorDraft();
        pendingRefresh = true;
        return true;
      },
      beginWrite: () => {
        if (editorBusy) throw new Error('Wait for the current save.');
        return freezeEditor();
      },
      onConflict: (error) => notice(error.message || String(error), true),
    });
    metadataCleanup = mountTaskMetadata({
      taskId: scheduledTaskId,
      accountId: account,
      csrf,
      canWrite,
      container: $('#metadata-container'),
      getRevision: () =>
        editor?.id === scheduledTaskId && !editorBusy ? editor.revision : null,
      onSaved: (expected, revision) => {
        if (editor?.id !== scheduledTaskId || editor.revision !== expected) return false;
        editor.revision = revision;
        if (sessionStorage.getItem(`todoer:${account}:edit:${editor.id}`))
          saveEditorDraft();
        pendingRefresh = true;
        return true;
      },
      onTaskChanged: (task) => {
        if (
          editor?.id === scheduledTaskId &&
          task.id === scheduledTaskId &&
          task.revision === editor.revision
        )
          editor = { ...editor, ...task };
      },
      hasUnsavedBrief: () =>
        editor?.id !== scheduledTaskId ||
        $('#edit-title').value !== editor.title ||
        $('#edit-description').value !== editor.description,
      beginWrite: () => {
        if (editorBusy) throw new Error('Wait for the current save.');
        return freezeEditor();
      },
      onConflict: (error) => notice(error.message || String(error), true),
    });
    $('#move-destination').value = editor.list_id;
    pending?.finish();
    if (!dialog.open) dialog.showModal();
    if (push)
      history.pushState({ task: editor.id, todoerPanel: true }, '', `?task=${editor.id}`);
    $('#edit-title').focus();
  } catch (err) {
    notice(err.message, true);
  } finally {
    pending?.finish();
    if (sequence === editorOpenSequence) editorOpening = false;
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
    complete.disabled = !canWrite;
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
      ? 'Open sub-tasks →'
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
    if (editor?.id === parent && $('#subtask-title').value.trim() === title)
      $('#subtask-title').value = '';
    childCapture = null;
    const children = await api(`/tasks/${parent}/subtree`);
    if (editor?.id === parent) renderChildren(children);
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
  if (!editor || editorBusy) return;
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
  const task = { ...editor };
  const release = freezeEditor();
  let closed = false;
  try {
    const preview = await api(
      `/tasks/${task.id}/move-preview?destination=${encodeURIComponent(destination)}`,
    );
    let acknowledge_access_change = false;
    if (preview.audience_broadens) {
      acknowledge_access_change = await confirmAction(
        preview.destination_public && !preview.source_public
          ? 'This move makes this task and all its sub-tasks publicly viewable. Continue?'
          : `This move gives ${preview.added_members} additional member(s) access to this task and all its sub-tasks. Allow this access change?`,
      );
      if (!acknowledge_access_change) return;
    }
    await api(`/tasks/${task.id}/move`, 'POST', {
      destination,
      revision: task.revision,
      acknowledge_access_change,
    });
    release();
    if (dialog.open && editor?.id === task.id) {
      closeEditor();
      closed = true;
    }
    notice('');
    refresh(true);
  } catch (err) {
    $('#edit-error').textContent = err.message;
  } finally {
    release();
    if (!closed) settleEditorRoute();
  }
};

mountTaskOrdering({
  container: $('#task-list'),
  listId: list,
  canWrite,
  request: api,
  onChanged: () => refresh(),
  onError: (message) => notice(message, true),
  hasActiveDraft: () =>
    dialog.open ||
    editorOpening ||
    inlineEditors.size > 0 ||
    Boolean(document.querySelector('#capture-title')?.value) ||
    Boolean(window.getSelection()?.toString()),
});
async function renameInline(row, snapshot) {
  const id = row.dataset.id;
  if (inlineEditors.has(id)) return;
  inlineEditors.set(id, null);
  try {
    const task = snapshot || (await api(`/tasks/${id}/subtree`)).find((t) => t.id === id);
    if (!task) {
      inlineEditors.delete(id);
      return;
    }
    if (!row.isConnected) row = $$('#task-list .task').find((r) => r.dataset.id === id);
    if (!row) {
      inlineEditors.delete(id);
      return;
    }
    const display = row.querySelector('.task-title');
    const form = document.createElement('form');
    form.className = 'inline-editor';
    const input = document.createElement('input');
    input.value = task.title;
    input.required = true;
    input.maxLength = 500;
    input.setAttribute('aria-label', 'Task title');
    const save = document.createElement('button');
    save.type = 'submit';
    save.textContent = 'Save';
    const cancel = document.createElement('button');
    cancel.type = 'button';
    cancel.textContent = 'Cancel';
    const key = `todoer:${account}:inline:${id}`;
    const stash = () =>
      sessionStorage.setItem(key, JSON.stringify({ ...task, title: input.value }));
    input.oninput = stash;
    inlineEditors.set(id, form);
    display.hidden = true;
    form.append(input, save, cancel);
    display.before(form);
    const close = () => {
      sessionStorage.removeItem(key);
      form.remove();
      display.hidden = false;
      inlineEditors.delete(id);
      display.focus();
      if (pendingRefresh) refresh();
    };
    cancel.onclick = close;
    input.onkeydown = (e) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        close();
      }
    };
    form.onsubmit = async (e) => {
      e.preventDefault();
      save.disabled = true;
      input.disabled = true;
      cancel.disabled = true;
      form.setAttribute('aria-busy', 'true');
      try {
        const result = await api(`/tasks/${id}`, 'PATCH', {
          revision: task.revision,
          title: input.value,
        });
        display.textContent = result.title;
        row.dataset.revision = result.revision;
        close();
        notice('');
      } catch (err) {
        stash();
        notice(`${err.message}. Your inline draft is preserved.`, true);
      } finally {
        save.disabled = false;
        input.disabled = false;
        cancel.disabled = false;
        form.removeAttribute('aria-busy');
      }
    };
    $('#capture-title')?.focus();
    input.select();
  } catch (err) {
    if (inlineEditors.get(id) === null) inlineEditors.delete(id);
    notice(err.message, true);
  }
}
for (const row of $$('#task-list .task')) {
  const saved = sessionStorage.getItem(`todoer:${account}:inline:${row.dataset.id}`);
  if (saved) {
    try {
      renameInline(row, JSON.parse(saved));
    } catch {}
  }
}

document.body.dataset.ready = 'true';

mountListUi({ api, notice, listId: list, csrf });
