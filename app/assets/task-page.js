import { mountSmartCapture } from './smart-capture.js';
import { beginLoading } from './loading.js';
import { request as fetch } from './request.js';
import { editorAnchor, alignEditorField } from './seamless-editor.js';
import { preferredTimeFormatter } from './preferred-time.js';
import { stowInfo, setSaveState, trackSave } from './quiet-info.js';
import { toast } from './toast.js';
import { confirmAction } from './confirm.js';
import './site.js';
import { mountBreakdown } from './task-breakdown.js';
mountBreakdown(document.querySelector('[data-task-page]'));
import { mountDiscussion } from './discussion.js';
import { mountScheduling } from './scheduling.js';
import { mountTaskMetadata } from './task-metadata.js';
import { openTaskSettings, setupTaskSettingsDock } from './list-settings.js';

// Shared by task-page and list-row move dialogs. Both use the same server
// dry-run plans; the server supplies the next revision for section assignment.
export async function previewTaskMove(
  api,
  task,
  destination,
  sectionId,
  acknowledgeAccessChange = false,
) {
  const operations = [];
  if (destination !== task.list_id) {
    const move = {
      op: 'move',
      id: task.id,
      revision: task.revision,
      destination,
      acknowledge_access_change: acknowledgeAccessChange,
    };
    operations.push(move);
    if (sectionId) {
      const first = await api('/bulk/preview', 'POST', { operations: [move] });
      const after = first.steps[0].after.find((value) => value.id === task.id);
      if (!after) throw new Error('Move preview did not return the root task.');
      operations.push({
        op: 'section',
        id: task.id,
        revision: after.revision,
        section_id: sectionId,
      });
    }
  } else {
    if (task.parent_id)
      throw new Error(
        'This child inherits its parent section. Reparent it before assigning a section.',
      );
    operations.push({
      op: 'section',
      id: task.id,
      revision: task.revision,
      section_id: sectionId || null,
    });
  }
  return api('/bulk/preview', 'POST', { operations });
}

function checkIcon(button, complete) {
  button.replaceChildren();
  if (!complete) return;
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('class', 'ui-icon');
  svg.setAttribute('width', '14');
  svg.setAttribute('height', '14');
  svg.setAttribute('aria-hidden', 'true');
  const use = document.createElementNS('http://www.w3.org/2000/svg', 'use');
  use.setAttribute('href', '/assets/icons.svg#check');
  svg.append(use);
  button.append(svg);
}

export function mountTaskPage(root, { embedded = false } = {}) {
  const lifecycle = new AbortController();
  const get = (selector) => root.querySelector(selector);
  const id = root.dataset.taskId,
    account = root.dataset.accountId,
    csrf = root.dataset.csrf;
  const prefix = `todoer:${account}:edit:${id}`,
    childKey = `${prefix}:child`;
  const form = get('[data-brief-form]'),
    title = get('#task-page-title'),
    description = get('#task-page-description');
  for (const [selector, label, icon] of [
    ['[data-context-edit]', 'Edit task…', 'edit'],
    ['[data-open-move]', 'Move task and sub-tasks…', 'move'],
    ['[data-open-advanced]', 'Task settings…', 'settings'],
    ['[data-trash]', 'Move to trash…', 'trash'],
  ]) {
    const button = get(selector);
    button.querySelector('span').textContent = label;
    button.querySelector('use').setAttribute('href', `/assets/icons.svg#${icon}`);
  }
  form.prepend(title);
  title.placeholder = 'Task name';
  description.placeholder = 'Description (Markdown supported)';
  const editActions = form.querySelector('.task-page-actions');
  const cancelEdit = form.querySelector('[data-cancel-edit]');
  cancelEdit.replaceChildren(document.createTextNode('Cancel'));
  const saveEdit = form.querySelector('[type=submit]');
  saveEdit.replaceChildren(document.createTextNode('Save'));
  editActions.prepend(cancelEdit, saveEdit);
  const canWrite = root.dataset.canWrite === 'true';
  fetch(`/api/v1/lists/${encodeURIComponent(root.dataset.listId)}/color`)
    .then((r) => (r.ok ? r.json() : null))
    .then((value) => {
      const color = value?.color || 'blue';
      const dot = document.createElement('span');
      dot.className = `list-dot badge-${color}`;
      dot.hidden = true;
      root.append(dot);
      const resolved = /^#[0-9a-f]{6}$/i.test(color)
        ? color
        : getComputedStyle(dot).backgroundColor;
      root.style.setProperty('--list-color', resolved);
      dot.remove();
    })
    .catch(() => {});

  let task = {
    id,
    list_id: root.dataset.listId,
    parent_id: root.dataset.parentId || null,
    title: title.value,
    description: description.value,
    revision: Number(root.dataset.revision),
    complete: root.dataset.complete === 'true',
  };
  let draft = read(prefix),
    childDraft = read(childKey),
    busy = false,
    loading = false,
    pending = false,
    alive = true;
  let children = new Map(),
    nextCursor = null,
    snapshot = null,
    eventSource,
    pollTimer,
    refreshTimer,
    editOpen = false;
  let discussion,
    scheduling,
    metadata,
    movePreview = null,
    moveTask = null,
    moveKey = null,
    moveSequence = 0;
  const controllers = new Set();
  function notice(message, error = false) {
    if (!alive) return;
    const target = get('[data-task-notice]');
    if (!error && stowInfo(message, `task:${id}`)) {
      target.textContent = '';
      return;
    }
    target.textContent = message;
    target.classList.toggle('error', error);
    if (!error && /copied\.$/.test(message)) {
      target.classList.add('copy-toast');
      setTimeout(() => {
        if (target.textContent === message) {
          target.textContent = '';
          target.classList.remove('copy-toast');
        }
      }, 2500);
    } else target.classList.remove('copy-toast');
  }
  function read(key) {
    try {
      return JSON.parse(sessionStorage.getItem(key) || 'null');
    } catch {
      return null;
    }
  }
  function store(key, value) {
    try {
      if (value === null) sessionStorage.removeItem(key);
      else sessionStorage.setItem(key, JSON.stringify(value));
    } catch {
      notice(
        'Browser draft storage is unavailable. Copy your changes before leaving.',
        true,
      );
    }
  }
  function dirty() {
    return title.value !== task.title || description.value !== task.description;
  }
  function remember() {
    if (!canWrite || !alive) return;
    if (dirty()) {
      draft = {
        title: title.value,
        description: description.value,
        revision: draft?.revision ?? task.revision,
        base_title: draft?.base_title ?? task.title,
        base_description: draft?.base_description ?? task.description,
      };
      store(prefix, draft);
      setSaveState(
        'task:' + id,
        'unsaved',
        'Your task title and description changes are kept on this device until you save them.',
      );
    } else if (!draft || draft.revision === task.revision) {
      draft = null;
      store(prefix, null);
    }
  }
  function conflict(message) {
    if (!alive) return;
    get('[data-conflict]').textContent = message;
    get('[data-rebase]').hidden = !draft || draft.revision === task.revision;
    get('[data-discard]').hidden = !draft;
    notice(message, true);
  }
  function openEditor(field = 'title') {
    if (!canWrite || busy || !alive) return;
    const titleAnchor = editorAnchor(get('[data-edit-title]'));
    const descriptionAnchor = editorAnchor(get('.task-page-brief'));
    editOpen = true;
    root.dataset.editMode = 'combined';
    get('.task-context-menu').open = false;
    form.hidden = false;
    title.hidden = false;
    description.hidden = false;
    get('[data-edit-title]').hidden = true;
    if (draft) {
      title.value = draft.title;
      description.value = draft.description;
      get('[data-discard]').hidden = false;
    }
    alignEditorField(title, titleAnchor, { expand: true });
    alignEditorField(description, descriptionAnchor);
    (field === 'description' ? description : title).focus();
    if (field === 'title') title.select();
  }
  function closeEditor() {
    remember();
    editOpen = false;
    delete root.dataset.editMode;
    form.hidden = true;
    title.hidden = true;
    get('[data-edit-title]').hidden = false;
    notice('');
  }
  let pointerHeld = false;
  const interacting = () =>
    pointerHeld || Boolean(root.querySelector('.task-child-menu[open]'));
  root.addEventListener(
    'pointerdown',
    () => {
      pointerHeld = true;
    },
    { capture: true, signal: lifecycle.signal },
  );
  for (const type of ['pointerup', 'pointercancel'])
    window.addEventListener(
      type,
      () => {
        pointerHeld = false;
        setTimeout(releaseSoon, 0);
      },
      { capture: true, signal: lifecycle.signal },
    );
  root.addEventListener('toggle', releaseSoon, {
    capture: true,
    signal: lifecycle.signal,
  });
  function releaseSoon() {
    if (pending && !busy && !interacting() && alive) {
      clearTimeout(refreshTimer);
      refreshTimer = setTimeout(refresh, 0);
    }
  }
  function beginWrite() {
    if (!alive || !canWrite) throw new Error('This task is read-only.');
    if (busy) throw new Error('Wait for the current save. Your draft is preserved.');
    remember();
    busy = true;
    const disabled = new Map();
    for (const control of root.querySelectorAll('button,input,textarea,select')) {
      disabled.set(control, control.disabled);
      control.disabled = true;
    }
    let released = false;
    return () => {
      if (released) return;
      released = true;
      for (const [control, prior] of disabled)
        if (control.isConnected) control.disabled = prior;
      busy = false;
      releaseSoon();
    };
  }
  function saved(expected, revision) {
    if (!alive || task.revision !== expected || !Number.isSafeInteger(revision))
      return false;
    task.revision = revision;
    // A successful local CAS that changes only metadata/planning can advance
    // our brief base. Remote refreshes never advance a draft's original base.
    if (
      draft?.revision === expected &&
      draft.base_title === task.title &&
      draft.base_description === task.description
    ) {
      draft.revision = revision;
      store(prefix, draft);
    }
    pending = true;
    return true;
  }
  function request(path, method = 'GET', ...args) {
    const action = () => performRequest(path, method, ...args);
    return method === 'GET' || /preview|intents/.test(path)
      ? action()
      : trackSave('task:' + id, action);
  }
  async function performRequest(
    path,
    method = 'GET',
    body,
    { html = false, headers = {} } = {},
  ) {
    if (!alive) throw new Error('Task page closed.');
    const controller = new AbortController(),
      timer = setTimeout(() => controller.abort(), 30000);
    controllers.add(controller);
    try {
      const response = await fetch(path, {
        method,
        signal: controller.signal,
        headers: {
          ...(body ? { 'Content-Type': 'application/json' } : {}),
          ...(method !== 'GET' ? { 'X-CSRF-Token': csrf } : {}),
          ...headers,
        },
        body: body ? JSON.stringify(body) : undefined,
      });
      if (!response.ok) {
        const value = await response.json().catch(() => ({}));
        const error = new Error(
          value.error?.message || `Request failed (${response.status}).`,
        );
        error.status = response.status;
        error.code = value.error?.code;
        throw error;
      }
      const value = await (html ? response.text() : response.json());
      if (!alive) throw new Error('Task page closed.');
      return value;
    } finally {
      clearTimeout(timer);
      controllers.delete(controller);
    }
  }
  const api = (path, method, body, options) =>
    request(`/api/v1${path}`, method, body, options);
  let completionRevision;
  async function showCompletion() {
    root.classList.toggle('is-complete', task.complete);
    let label = root.querySelector('.task-completion-label');
    if (!task.complete) {
      label?.remove();
      completionRevision = null;
      return;
    }
    if (!label) {
      label = document.createElement('button');
      label.type = 'button';
      label.className = 'task-completion-label';
      label.textContent = 'Completed · View activity';
      label.onclick = () => openTaskSettings(root, 'activity');
      root.querySelector('.task-page-heading').after(label);
    }
    if (completionRevision === task.revision) return;
    completionRevision = task.revision;
    const revision = task.revision;
    try {
      let before, entry;
      do {
        const page = await api(
          `/tasks/${encodeURIComponent(id)}/activity${before ? `?before=${before}` : ''}`,
        );
        if (!alive || !task.complete || task.revision !== revision) return;
        entry = page.items.find((item) => item.completed);
        before = page.next_before;
      } while (!entry && before);
      if (entry) {
        const formatTime = await preferredTimeFormatter();
        if (alive && task.complete && task.revision === revision) {
          label.textContent = `Completed ${formatTime(entry.occurred_at)} by ${entry.actor}`;
          label.title = formatTime.timeZone;
        }
      }
    } catch {
      completionRevision = null;
    }
  }
  function updateView() {
    if (!alive) return;
    showCompletion();
    root.dataset.parentId = task.parent_id || '';
    const referenceButton = get('[data-copy-task-reference]');
    if (referenceButton) {
      const reference = `${task.parent_id ? '^' : '%'}${referenceButton.dataset.copyTaskReference.replace(/^[%^]/, '')}`;
      referenceButton.dataset.copyTaskReference = reference;
      referenceButton.textContent = reference;
      const crumb = get('.breadcrumbs [aria-current="page"]');
      if (crumb) crumb.textContent = reference;
    }
    let promote = get('.task-context-menu [data-task-promote]');
    if (!promote) {
      promote = document.createElement('button');
      promote.type = 'button';
      promote.dataset.taskPromote = id;
      promote.innerHTML =
        '<svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#arrow-up"/></svg><span>Move to parent…</span>';
      get('.task-context-menu > div').append(promote);
    }
    promote.hidden = !canWrite || !task.parent_id;
    get('[data-edit-title]').textContent = task.title;
    if (!embedded) document.title = `${task.title} · Todoer`;
    const complete = get('[data-root-complete]');
    checkIcon(complete, task.complete);
    complete.setAttribute('aria-pressed', String(task.complete));
    complete.setAttribute('aria-label', task.complete ? 'Reopen task' : 'Complete task');
  }
  async function fetchTask() {
    const page = await api(`/tasks/${encodeURIComponent(id)}/subtree/query`, 'POST', {
      limit: 1,
      fields: [
        'reference',
        'list_id',
        'title',
        'description',
        'complete',
        'revision',
        'position',
      ],
    });
    const value = page.items[0];
    if (!value || value.id !== id) throw new Error('Task is no longer available.');
    return value;
  }
  function projection(cursor = null) {
    return { limit: 100, fields: ['title', 'complete', 'revision', 'position'], cursor };
  }
  function renderChildren() {
    const container = get('[data-children]');
    container.replaceChildren();
    const byParent = new Map();
    for (const child of children.values()) {
      const group = byParent.get(child.parent_id) || [];
      group.push(child);
      byParent.set(child.parent_id, group);
    }
    const leafCounts = (nodeId) => {
      const descendants = byParent.get(nodeId) || [];
      if (!descendants.length) {
        const child = children.get(nodeId);
        return [child?.complete ? 1 : 0, 1];
      }
      return descendants
        .map((child) => leafCounts(child.id))
        .reduce((a, b) => [a[0] + b[0], a[1] + b[1]], [0, 0]);
    };
    const loadedCounts = (nodeId) => {
      const descendants = byParent.get(nodeId) || [];
      const own = children.get(nodeId);
      return descendants
        .map((child) => loadedCounts(child.id))
        .reduce(
          (a, b) => [a[0] + b[0], a[1] + b[1]],
          own ? [own.complete ? 1 : 0, 1] : [0, 0],
        );
    };
    function append(parent, target) {
      for (const child of byParent.get(parent) || []) {
        const row = document.createElement('li');
        row.dataset.childId = child.id;
        row.className = 'task-page-child';
        const line = document.createElement('div');
        line.className = 'task-page-child-line';
        const complete = document.createElement('button');
        complete.type = 'button';
        complete.disabled = !canWrite || busy;
        complete.className = 'task-page-complete task-child-complete';
        checkIcon(complete, child.complete);
        complete.setAttribute(
          'aria-label',
          `${child.complete ? 'Reopen' : 'Complete'} ${child.title}`,
        );
        complete.setAttribute('aria-pressed', String(child.complete));
        complete.onclick = () =>
          toggleComplete(child, (byParent.get(child.id) || []).length > 0);
        const link = document.createElement('a');
        link.href = `/tasks/${encodeURIComponent(child.id)}`;
        link.textContent = child.title;
        line.append(complete, link);
        if (canWrite) {
          const menu = document.createElement('details');
          menu.className = 'entity-menu task-child-menu';
          const summary = document.createElement('summary');
          summary.setAttribute('aria-label', `Actions for ${child.title}`);
          summary.innerHTML =
            '<svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#more"/></svg>';
          const actions = document.createElement('div');
          actions.className = 'menu-items';
          const exportButton = document.createElement('button');
          exportButton.type = 'button';
          exportButton.textContent = 'Export…';
          exportButton.dataset.exportKind = 'task';
          exportButton.dataset.exportId = child.id;
          actions.append(exportButton);
          const promote = document.createElement('button');
          promote.type = 'button';
          promote.dataset.taskPromote = child.id;
          promote.textContent = 'Move to parent…';
          actions.append(promote);
          menu.append(summary, actions);
          line.append(menu);
        }

        const nested = byParent.get(child.id) || [];
        if (nested.length) {
          const counts = nextCursor ? loadedCounts(child.id) : leafCounts(child.id);
          const progress = document.createElement('small');
          progress.className = 'task-child-progress';
          progress.textContent = `${counts[0]}/${counts[1]} ${nextCursor ? 'loaded items' : 'leaf items'}`;
          line.append(progress);
        }
        const childMenu = line.querySelector('.task-child-menu');
        if (childMenu) line.append(childMenu);
        row.append(line);
        if (nested.length) {
          const list = document.createElement('ol');
          row.append(list);
          append(child.id, list);
        }
        target.append(row);
      }
    }
    append(id, container);
    const roots = byParent.get(id) || [],
      counts = roots
        .map((child) => leafCounts(child.id))
        .reduce((a, b) => [a[0] + b[0], a[1] + b[1]], [0, 0]);
    get('[data-child-progress]').textContent = counts[1]
      ? nextCursor
        ? `${[...children.values()].filter((item) => item.complete).length}/${children.size} loaded items complete; load remaining items for leaf progress`
        : `${counts[0]}/${counts[1]} tasks complete`
      : '';
    let completionLine = get('[data-subtask-completion-line]');
    if (!completionLine) {
      completionLine = document.createElement('progress');
      completionLine.dataset.subtaskCompletionLine = '';
      completionLine.className = 'space-completion-line task-subtask-completion-line';
      completionLine.setAttribute('aria-label', 'Sub-task completion');
      get('[data-child-progress]').closest('.section-heading').after(completionLine);
    }
    completionLine.hidden = !!nextCursor || !counts[1];
    completionLine.max = counts[1] || 1;
    completionLine.value = counts[0];
    completionLine.title = `${counts[0]}/${counts[1]} tasks complete`;
    let percent = get('[data-child-progress-percent]');
    if (!percent) {
      percent = document.createElement('span');
      percent.dataset.childProgressPercent = '';
      get('[data-child-progress]').after(percent);
    }
    percent.textContent =
      !nextCursor && counts[1] ? `${Math.round((counts[0] / counts[1]) * 100)}%` : '';
    get('[data-load-more]').hidden = !nextCursor;
  }
  async function loadChildren(cursor = null) {
    const page = await api(
      `/tasks/${encodeURIComponent(id)}/subtree/query`,
      'POST',
      projection(cursor),
    );
    if (!alive || busy || interacting()) {
      pending = true;
      return;
    }
    if (!cursor) {
      children = new Map();
      snapshot = page.snapshot;
    } else if (JSON.stringify(snapshot) !== JSON.stringify(page.snapshot))
      throw new Error('Sub-tasks changed. Refresh before continuing.');
    for (const item of page.items) if (item.id !== id) children.set(item.id, item);
    nextCursor = page.next_cursor;
    renderChildren();
  }
  async function refresh() {
    if (!alive) return;
    if (busy || loading || interacting()) {
      pending = true;
      return;
    }
    loading = true;
    pending = false;
    get('[data-connection]').textContent = 'Refreshing...';
    get('[data-load-more]').disabled = true;
    try {
      const current = await fetchTask();
      if (!alive) return;
      if (busy || interacting()) {
        pending = true;
        return;
      }
      if (current.list_id !== task.list_id) {
        remember();
        location.reload();
        return;
      }
      const hasDraft = !!draft || dirty();
      remember();
      task = { ...task, ...current };
      updateView();
      if (!hasDraft) {
        title.value = task.title;
        description.value = task.description;
      } else if (draft && draft.revision !== task.revision)
        conflict(
          'The saved task changed. Your draft and its original revision are preserved. Review the saved version before rebasing.',
        );
      const rendered = await request(
        `/fragments/tasks/${encodeURIComponent(id)}/description`,
        'GET',
        undefined,
        { html: true },
      );
      if (!alive) return;
      if (busy || interacting()) {
        pending = true;
        return;
      }
      get('[data-saved-description]').innerHTML = rendered; // authenticated sanitized saved-source fragment
      await loadChildren();
      discussion?.refresh?.();
      scheduling?.refresh?.();
      metadata?.refresh?.();
      get('[data-connection]').textContent = 'Up to date';
    } catch (error) {
      if ([401, 403, 404].includes(error.status))
        revoke(
          error.status === 401
            ? 'Your session ended. Sign in again.'
            : 'This task was removed or your access changed.',
        );
      else if (alive)
        notice(error.message || 'Reconnect to refresh. Your draft is preserved.', true);
    } finally {
      loading = false;
      if (alive) get('[data-load-more]').disabled = busy;
      releaseSoon();
    }
  }
  function revoke(message, preserveDraftNotice = true) {
    if (!alive) return;
    remember();
    cleanup();
    const heading = document.createElement('h1');
    heading.textContent = 'Task unavailable';
    const text = document.createElement('p');
    text.textContent = preserveDraftNotice
      ? `${message} Unsaved drafts remain on this device under your account.`
      : message;
    const link = document.createElement('a');
    link.href = '/';
    link.textContent = 'Open your tasks';
    root.replaceChildren(heading, text, link);
    if (embedded) return;
    document.title = 'Task unavailable · Todoer';
    document.querySelector('nav[aria-label="Lists"]')?.replaceChildren();
    const workspaceCaption = document.querySelector('.account-caption small');
    if (workspaceCaption) workspaceCaption.textContent = 'Access changed';
    for (const link of document.querySelectorAll('.account-menu a[href^="/workspaces/"]'))
      link.remove();
  }
  async function toggleComplete(target, hasChildren = false, forceSubtree = false) {
    if (!canWrite || busy) return;
    const complete = forceSubtree || !target.complete;
    const subtree =
      complete && (forceSubtree || hasChildren || (target.id === id && !!nextCursor));
    if (
      subtree &&
      !(await confirmAction(
        'Complete this task and every unfinished item in its sub-tasks?',
      ))
    )
      return;
    const release = beginWrite();
    try {
      const updated = await api(`/tasks/${encodeURIComponent(target.id)}`, 'PATCH', {
        revision: target.revision,
        complete,
        complete_subtree: subtree,
      });
      if (target.id === id) {
        saved(target.revision, updated.revision);
        task = { ...task, ...updated };
        updateView();
      }
      pending = true;
      notice('');
    } catch (error) {
      conflict(error.message);
      pending = true;
    } finally {
      release();
    }
  }
  for (const selector of ['[data-edit-title]', '[data-context-edit]'])
    get(selector).onclick = () => openEditor('title');
  get('[data-edit-description]')?.addEventListener('click', () =>
    openEditor('description'),
  );
  get('[data-saved-description]').addEventListener('click', (event) => {
    if (!event.target.closest('a,button')) openEditor('description');
  });
  get('[data-saved-description]').tabIndex = canWrite ? 0 : -1;
  get('[data-saved-description]').addEventListener('keydown', (event) => {
    if (event.target === event.currentTarget && ['Enter', ' '].includes(event.key)) {
      event.preventDefault();
      openEditor('description');
    }
  });
  for (const input of [title, description]) input.addEventListener('input', remember);
  get('[data-cancel-edit]').onclick = closeEditor;
  get('[data-copy-task-reference]')?.addEventListener('click', async (event) => {
    try {
      await navigator.clipboard.writeText(event.currentTarget.dataset.copyTaskReference);
      toast('Reference copied.');
    } catch {
      notice('Could not copy reference.', true);
    }
  });
  get('[data-open-advanced]').onclick = () => {
    get('.task-context-menu').open = false;
    openTaskSettings(root);
  };
  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (busy || !canWrite) return;
    remember();
    const writing = {
      title: title.value,
      description: description.value,
      revision: draft?.revision ?? task.revision,
    };
    const release = beginWrite();
    try {
      const result = await api(`/tasks/${encodeURIComponent(id)}`, 'PATCH', writing);
      task = { ...task, ...result };
      draft = null;
      store(prefix, null);
      title.value = result.title;
      description.value = result.description;
      get('[data-conflict]').textContent = '';
      get('[data-rebase]').hidden = true;
      get('[data-discard]').hidden = true;
      editOpen = false;
      delete root.dataset.editMode;
      form.hidden = true;
      title.hidden = true;
      get('[data-edit-title]').hidden = false;
      updateView();
      notice('');
      pending = true;
    } catch (error) {
      conflict(error.message);
      pending = true;
    } finally {
      release();
    }
  });
  get('[data-rebase]').onclick = async () => {
    if (
      busy ||
      !draft ||
      !(await confirmAction(
        'Keep your draft text and use the latest saved revision? Saving afterward will replace the saved title and description with your draft.',
      ))
    )
      return;
    const release = beginWrite();
    try {
      const current = await fetchTask();
      task = { ...task, ...current };
      draft.revision = current.revision;
      draft.base_title = current.title;
      draft.base_description = current.description;
      store(prefix, draft);
      updateView();
      get('[data-rebase]').hidden = true;
      get('[data-conflict]').textContent =
        'Your draft now uses the latest revision. Review it before saving.';
      pending = true;
    } catch (error) {
      conflict(error.message);
    } finally {
      release();
    }
  };
  get('[data-discard]').onclick = async () => {
    if (busy || !(await confirmAction('Discard this unsaved task draft?'))) return;
    draft = null;
    store(prefix, null);
    title.value = task.title;
    description.value = task.description;
    get('[data-conflict]').textContent = '';
    get('[data-rebase]').hidden = true;
    get('[data-discard]').hidden = true;
    closeEditor();
    notice('');
  };
  get('[data-root-complete]').onclick = () => toggleComplete(task, children.size > 0);
  get('[data-complete-all]').onclick = () => toggleComplete(task, true, true);
  get('[data-permalink]').onclick = async () => {
    try {
      await navigator.clipboard.writeText(new URL(`/tasks/${id}`, location.origin).href);
      toast('Task permalink copied.');
    } catch {
      notice(`Task permalink: ${new URL(`/tasks/${id}`, location.origin).href}`);
    }
  };
  get('[data-trash]').onclick = async () => {
    if (
      !canWrite ||
      busy ||
      !(await confirmAction(
        'Move this task and all its sub-tasks to trash? Unsaved drafts remain on this device.',
      ))
    )
      return;
    const release = beginWrite();
    try {
      await api(`/tasks/${encodeURIComponent(id)}/trash`, 'POST', {
        revision: task.revision,
      });
      location.assign(`/lists/${encodeURIComponent(task.list_id)}`);
    } catch (error) {
      conflict(error.message);
      pending = true;
    } finally {
      release();
    }
  };
  const childForm = get('[data-child-form]'),
    childTitle = get('#task-child-title'),
    childDescription = get('#task-child-description');
  if (childDraft) {
    childTitle.value = childDraft.title || '';
    childDescription.value = childDraft.description || '';
    if (childTitle.value || childDescription.value)
      get('[data-child-composer]').open = true;
  }
  function rememberChild() {
    if (!canWrite || !alive) return;
    if (!childTitle.value && !childDescription.value) {
      childDraft = null;
      store(childKey, null);
      return;
    }
    const same =
      childDraft?.title === childTitle.value &&
      childDraft?.description === childDescription.value;
    childDraft = {
      title: childTitle.value,
      description: childDescription.value,
      key: same ? childDraft.key : crypto.randomUUID(),
    };
    store(childKey, childDraft);
  }
  get('[data-child-cancel]').addEventListener('click', () => {
    rememberChild();
    get('[data-child-composer]').open = false;
    get('[data-child-composer] > summary').focus();
  });
  const grow = (input) => {
    input.style.height = 'auto';
    input.style.height = `${input.scrollHeight + input.offsetHeight - input.clientHeight}px`;
  };
  get('[data-child-composer]').addEventListener('toggle', () => {
    if (get('[data-child-composer]').open) {
      childTitle.focus();
      grow(childDescription);
    }
  });
  childDescription.addEventListener('input', () => grow(childDescription));
  const childCapture = mountSmartCapture(childForm);
  childForm.addEventListener('input', rememberChild);
  childForm.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (busy || !canWrite) return;
    rememberChild();
    await childCapture.ready;
    const interpreted = childCapture.get();
    if (!interpreted.title) return;
    const writing = { ...childDraft };
    const release = beginWrite();
    try {
      await api(
        '/capture',
        'POST',
        {
          list_id: task.list_id,
          metadata: interpreted.metadata,
          tasks: [
            { title: interpreted.title, description: writing.description, parent_id: id },
          ],
        },
        { headers: { 'Idempotency-Key': writing.key } },
      );
      if (
        childTitle.value === writing.title &&
        childDescription.value === writing.description
      ) {
        childTitle.value = '';
        childCapture.reset();
        childDescription.value = '';
        childDraft = null;
        store(childKey, null);
      }
      pending = true;
      notice('');
    } catch (error) {
      notice(error.message, true);
    } finally {
      release();
      childTitle.focus();
    }
  });
  get('[data-load-more]').onclick = async () => {
    if (busy || loading || !nextCursor) return;
    const button = get('[data-load-more]');
    button.disabled = true;
    loading = true;
    try {
      await loadChildren(nextCursor);
    } catch (error) {
      notice(`${error.message} Your draft is safe; refreshing the sub-tasks.`, true);
      pending = true;
    } finally {
      loading = false;
      button.disabled = busy;
      releaseSoon();
    }
  };
  const moveDialog = get('[data-move-dialog]'),
    moveForm = get('[data-move-form]'),
    moveList = get('#task-move-list'),
    moveSection = get('#task-move-section'),
    ack = get('[data-move-ack]');
  function invalidateMove() {
    movePreview = null;
    moveKey = null;
    get('[data-apply-move]').hidden = true;
    get('[data-move-error]').textContent = '';
  }
  async function inspectDestination() {
    const sequence = ++moveSequence;
    invalidateMove();
    ack.checked = false;
    get('[data-move-ack-label]').hidden = true;
    if (!moveList.value) return;
    const skeleton = beginLoading(moveForm, { variant: 'form' });
    try {
      const [sections, audience] = await Promise.all([
        api(`/lists/${encodeURIComponent(moveList.value)}/sections`),
        api(
          `/tasks/${encodeURIComponent(id)}/move-preview?destination=${encodeURIComponent(moveList.value)}`,
        ),
      ]);
      if (sequence !== moveSequence || !alive) return;
      moveSection.replaceChildren(new Option('No section', ''));
      for (const section of sections)
        moveSection.append(new Option(section.title, section.id));
      const source = moveTask ?? task;
      if (moveList.value === source.list_id) moveSection.value = root.dataset.sectionId;
      moveSection.disabled = moveList.value === source.list_id && !!source.parent_id;
      get('[data-move-audience]').textContent =
        audience.destination_public && !audience.source_public
          ? 'This move makes this task and all its sub-tasks publicly viewable. Review before moving.'
          : audience.audience_broadens
            ? `This destination adds access for ${audience.added_members} member(s). Review this before moving the all sub-tasks.`
            : 'This destination adds no members who can access these sub-tasks.';
      get('[data-move-ack-label]').hidden = !audience.audience_broadens;
      if (moveSection.disabled)
        get('[data-move-audience]').textContent +=
          ' This child inherits its parent section. Choose another list, or use metadata to reparent it first.';
    } catch (error) {
      if (sequence === moveSequence) skeleton.fail(error.message, inspectDestination);
      get('[data-move-error]').textContent = error.message;
    } finally {
      skeleton.finish();
    }
  }
  get('[data-open-move]').onclick = async () => {
    if (busy || !canWrite) return;
    invalidateMove();
    moveTask = { ...task };
    get('[data-move-error]').textContent = '';
    moveDialog.showModal();
    const skeleton = beginLoading(moveForm, { variant: 'form' });
    try {
      const location = await api(`/tasks/${encodeURIComponent(id)}/section`);
      root.dataset.sectionId = location.section_id || '';
      moveList.value = moveTask.list_id;
      await inspectDestination();
    } catch (error) {
      skeleton.fail(error.message, () => {
        moveDialog.close();
        get('[data-open-move]').click();
      });
      get('[data-move-error]').textContent = error.message;
    } finally {
      skeleton.finish();
    }
  };
  moveList.addEventListener('change', inspectDestination);
  moveSection.addEventListener('change', invalidateMove);
  ack.addEventListener('change', invalidateMove);
  moveForm.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (busy || !canWrite) return;
    if (!get('[data-move-ack-label]').hidden && !ack.checked) {
      get('[data-move-error]').textContent =
        'Review and acknowledge the added access before continuing.';
      return;
    }
    const release = beginWrite();
    try {
      movePreview = await previewTaskMove(
        api,
        moveTask ?? task,
        moveList.value,
        moveSection.value,
        ack.checked,
      );
      const audience = movePreview.steps.find(
        (step) => step.access_change,
      )?.access_change;
      if (audience) {
        get('[data-move-audience]').textContent =
          audience.destination_public && !audience.source_public
            ? 'This move makes this task and all its sub-tasks publicly viewable. Review before moving.'
            : audience.audience_broadens
              ? `Reviewed plan adds access for ${audience.added_members} member(s). Review this current audience before applying the exact plan.`
              : 'The reviewed plan adds no members who can access these sub-tasks.';
      }
      moveKey = crypto.randomUUID();
      get('[data-apply-move]').hidden = false;
      get('[data-move-error]').textContent =
        'Move reviewed. Apply this exact plan within ten minutes; changes to permissions or revisions require a new review.';
    } catch (error) {
      invalidateMove();
      get('[data-move-error]').textContent = error.message;
    } finally {
      release();
    }
  });
  get('[data-apply-move]').onclick = async () => {
    if (busy || !movePreview) return;
    if (!get('[data-move-ack-label]').hidden && !ack.checked) {
      get('[data-move-error]').textContent =
        'Acknowledge the reviewed audience before applying.';
      return;
    }
    const plan = movePreview,
      release = beginWrite();
    try {
      await api('/bulk/apply', 'POST', {
        preview_id: plan.preview_id,
        expected_hash: plan.expected_hash,
        idempotency_key: moveKey,
      });
      remember();
      location.assign(`/tasks/${encodeURIComponent(id)}`);
    } catch (error) {
      get('[data-move-error]').textContent =
        `${error.message} Review again if this plan is stale.`;
    } finally {
      release();
    }
  };
  get('.task-context-menu').addEventListener('click', (event) => {
    if (event.target.closest('button')) get('.task-context-menu').open = false;
  });
  function mountControllers() {
    discussion = mountDiscussion({
      taskId: id,
      csrf,
      container: get('[data-discussion]'),
      accountId: account,
      onConflict: (error) => notice(error.message, true),
    });
    const shared = {
      taskId: id,
      accountId: account,
      csrf,
      canWrite,
      getRevision: () => (!busy && alive ? task.revision : null),
      onSaved: saved,
      beginWrite,
      onConflict: (error) => {
        notice(error.message || String(error), true);
        pending = true;
        releaseSoon();
      },
    };
    // A successful local CAS invalidates the other editor's in-flight snapshot.
    // Recreate that controller from its account-scoped draft, preserving disclosure
    // state, so an older read cannot re-enable controls with the previous revision.
    const remount = (selector, previous, create) => {
      const container = get(selector);
      const open = [...container.querySelectorAll('details')].map((d) => d.open);
      previous?.();
      const controller = create(container);
      [...container.querySelectorAll('details')].forEach((d, i) => {
        if (open[i]) d.open = true;
      });
      return controller;
    };
    const mountPlanning = () => {
      scheduling = remount('[data-scheduling]', scheduling, (container) =>
        mountScheduling({
          ...shared,
          container,
          onSaved: (expected, revision) => {
            if (!saved(expected, revision)) return false;
            mountMetadata();
            return true;
          },
        }),
      );
    };
    const mountMetadata = () => {
      metadata = remount('[data-metadata]', metadata, (container) =>
        mountTaskMetadata({
          ...shared,
          container,
          onSaved: (expected, revision) => {
            if (!saved(expected, revision)) return false;
            mountPlanning();
            return true;
          },
          hasUnsavedBrief: () => dirty() || !!draft,
          onTaskChanged: (current) => {
            if (current.id === id && current.revision === task.revision) {
              task = { ...task, ...current };
              root.dataset.sectionId = current.section_id ?? root.dataset.sectionId;
              updateView();
            }
          },
        }),
      );
    };
    mountPlanning();
    mountMetadata();
  }
  root.addEventListener('keydown', (event) => {
    if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
      if (event.target === title || event.target === description) {
        event.preventDefault();
        form.requestSubmit();
      } else if (childForm.contains(event.target)) {
        event.preventDefault();
        childForm.requestSubmit();
      }
    } else if (
      event.key === 'Escape' &&
      editOpen &&
      (event.target === title || event.target === description)
    ) {
      event.preventDefault();
      event.stopPropagation();
      closeEditor();
    } else if (event.key === 'Escape' && childForm.contains(event.target)) {
      event.preventDefault();
      event.stopPropagation();
      rememberChild();
      get('[data-child-composer]').open = false;
      notice('Your sub-task draft is kept on this device.');
    }
  });
  document.addEventListener(
    'keydown',
    (event) => {
      if (!alive || (embedded && !root.contains(event.target))) return;
      if (
        event.defaultPrevented ||
        event.metaKey ||
        event.ctrlKey ||
        event.altKey ||
        event.target.closest('input,textarea,select,[contenteditable="true"],dialog')
      )
        return;
      if (event.key === 'e') {
        event.preventDefault();
        openEditor('title');
      } else if (event.key === 'n' && canWrite) {
        event.preventDefault();
        get('[data-child-composer]').open = true;
        childTitle.focus();
      } else if (event.key === 'x') {
        event.preventDefault();
        toggleComplete(task, children.size > 0);
      } else if (event.key === 'Escape' && editOpen) {
        event.preventDefault();
        closeEditor();
      }
    },
    { signal: lifecycle.signal },
  );
  function cleanup() {
    if (!alive) return;
    remember();
    rememberChild();
    lifecycle.abort();
    for (const panel of root.querySelectorAll('dialog[open]')) panel.close();
    discussion?.();
    scheduling?.();
    metadata?.();
    alive = false;
    eventSource?.close();
    clearTimeout(pollTimer);
    clearTimeout(refreshTimer);
    for (const controller of controllers) controller.abort();
    controllers.clear();
  }
  if (
    draft &&
    typeof draft.title === 'string' &&
    typeof draft.description === 'string' &&
    Number.isSafeInteger(draft.revision)
  ) {
    title.value = draft.title;
    description.value = draft.description;
    openEditor(draft.title === task.title ? 'description' : 'title');
    conflict(
      draft.revision === task.revision
        ? 'Restored your unsaved task draft. Saving checks its original revision.'
        : 'Restored your task draft with an older revision. Review the saved version before rebasing.',
    );
  } else draft = null;
  mountControllers();
  if (embedded) mountBreakdown(root, { inline: true });
  if (!embedded) setupTaskSettingsDock(root);
  if (new URLSearchParams(location.search).get('move') === '1')
    get('[data-open-move]').click();
  refresh();
  if ('EventSource' in window) {
    eventSource = new EventSource(
      `/api/v1/events?after=${Number(root.dataset.cursor) || 0}&list_id=${encodeURIComponent(task.list_id)}`,
    );
    eventSource.addEventListener('changes', (event) => {
      let change;
      try {
        change = JSON.parse(event.data);
      } catch {
        return;
      }
      if (!change.resync && !(change.count > 0)) return;
      pending = true;
      releaseSoon();
    });
    eventSource.addEventListener('access-revoked', () =>
      revoke('Your list access was removed.'),
    );
    eventSource.addEventListener('session-expired', () =>
      revoke('Your session ended. Sign in again.'),
    );
    eventSource.onerror = () => {
      if (alive)
        get('[data-connection]').textContent = navigator.onLine
          ? 'Reconnecting'
          : 'Offline';
    };
  } else {
    const poll = async () => {
      await refresh();
      if (alive) pollTimer = setTimeout(poll, 5000);
    };
    pollTimer = setTimeout(poll, 5000);
  }
  document.addEventListener('todoer:logout', () => revoke('You signed out.', false), {
    signal: lifecycle.signal,
  });
  addEventListener(
    'online',
    () => {
      pending = true;
      releaseSoon();
    },
    { signal: lifecycle.signal },
  );
  addEventListener('pagehide', cleanup, { signal: lifecycle.signal });
  addEventListener(
    'pageshow',
    (event) => {
      if (event.persisted && !embedded) location.reload();
    },
    { signal: lifecycle.signal },
  );
  return cleanup;
}
const taskPage = document.querySelector('main[data-task-page]');
if (taskPage) mountTaskPage(taskPage);
