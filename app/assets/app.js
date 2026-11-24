import { pendingTaskRow } from './pending-task.js';
import { createTaskSwipeActions } from './task-swipe-actions.js';
import { trashTask } from './task-trash.js';
import { createTaskPanel } from './task-panel.js';
import { mountStructuredEdit } from './structured-edit.js';
import { reconcileCapturedTasks, reconcileTasks } from './task-list-refresh.js';

import { stowInfo } from './quiet-info.js';
import { confirmAction } from './confirm.js';
import { animateTaskCompletion, taskCompletionChange } from './task-completion.js';
import { mountCaptures } from './capture-ui.js';
import './site.js';
import { mountListUi } from './list-ui.js';
import { mountTaskOrdering } from './task-order.js';
import { mountSectionOrdering } from './section-order.js';
import { replayCaptures, listPending, clearAccount, discardCapture } from './offline.js';
import { clearPwa } from './pwa.js';
import { onPageLoad } from './page-lifecycle.js';

onPageLoad((scope) => {
  if (!document.querySelector('#task-list')) return;
  const fetch = scope.request;
  const main = document.querySelector('#main');
  const $ = (s) =>
    s.startsWith('meta[') ? document.querySelector(s) : main.querySelector(s);
  const csrf = $('meta[name="csrf-token"]').content;
  const list = document.body.dataset.list;
  const account = $('meta[name="account-id"]').content;
  const canWrite = document.body.dataset.canWrite === 'true';
  function restoreMainCapture() {
    const slot = document.querySelector('[data-main-capture-slot]');
    if (slot && !document.querySelector('#capture-form'))
      slot.append(
        document.querySelector('#main-capture-template').content.cloneNode(true),
      );
  }
  restoreMainCapture();
  scope.listen(document.body, 'htmx:after:swap', restoreMainCapture);
  let selectedTask;
  function highlightSelection(container = $('#task-list')) {
    if (!container) return;
    for (const row of container.querySelectorAll('.task'))
      row.classList.toggle('is-selected', row.dataset.id === selectedTask);
  }
  const taskPanel = createTaskPanel({
    csrf,
    onChanged: () => refresh(),
    onSelected: (id) => {
      selectedTask = id;
      highlightSelection();
    },
    onClose: resumeRefresh,
  });
  scope.cleanup(() => taskPanel.destroy());
  let taskPointerDown = false;
  // Do not replace a row between pointerdown and click. Firefox otherwise drops
  // a click when a live update removes its original event target.
  scope.listen(
    document,
    'pointerdown',
    (event) => {
      taskPointerDown = Boolean(event.target.closest?.('#task-list'));
    },
    true,
  );
  for (const type of ['pointerup', 'pointercancel'])
    scope.listen(
      document,
      type,
      () => {
        scope.timeout(() => {
          taskPointerDown = false;
          resumeRefresh();
        }, 0);
      },
      true,
    );
  let cursor = Number(document.body.dataset.cursor) || 0;
  let activeCaptures = 0;
  let pendingRefresh = false;
  const inlineEditors = new Map();
  const pendingRows = new Map();
  const confirmedCaptures = new Map();
  const captureRevisions = new Map();
  let restoreTaskFocus = null;
  const draftKey = `todoer:${account}:draft:${list}`;
  const input = $('#capture-title');
  input.value = sessionStorage.getItem(draftKey) || '';

  function notice(text, error = false) {
    if (scope.signal.aborted) return;
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
  function draftActive(ignorePointer = false) {
    return (
      (!ignorePointer && taskPointerDown) ||
      inlineEditors.size > 0 ||
      Boolean(
        $(
          '#task-list .task-actions[open],#task-list .entity-menu[open],#task-list [data-activation-pending],#task-list [data-completion-pending],#task-list [data-trash-pending]',
        ),
      ) ||
      [
        ...document.querySelectorAll(
          '.inline-capture:not([hidden]) input[name=title],.inline-capture:not([hidden]) textarea',
        ),
      ].some((field) => field.value.length > 0) ||
      activeCaptures > 0 ||
      Boolean(window.getSelection()?.toString())
    );
  }
  function resumeRefresh() {
    if (pendingRefresh && !draftActive()) refresh();
  }
  scope.listen(document, 'todoer:task-interaction-end', resumeRefresh);
  function refresh({ force = false, quiet = false } = {}) {
    if (scope.signal.aborted) return;
    if (inlineEditors.size || (!force && draftActive())) {
      pendingRefresh = true;
      if (!quiet) $('#refresh-notice').hidden = false;
      if (!confirmedCaptures.size) return;
    } else {
      pendingRefresh = false;
      $('#refresh-notice').hidden = true;
    }
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
    if (inlineEditors.size) notice('Finish or close the task editor first.');
    else refresh({ force: true });
  };
  scope.listen(document.body, 'htmx:before:request', ({ detail: { ctx } }) => {
    if (ctx.target?.id === 'task-list')
      ctx.todoerCapturedTasks = new Map(confirmedCaptures);
  });
  scope.listen(document.body, 'htmx:before:swap', (e) => {
    if (e.detail.ctx?.swap === 'none') return;
    const target = e.detail.ctx?.target;
    if (target?.id === 'task-list') {
      e.preventDefault();
      const blocked = draftActive();
      const captures = e.detail.ctx.todoerCapturedTasks;
      if (blocked) {
        pendingRefresh = true;
        if (!captures?.size) return;
      }
      const incoming = document.createElement('div');
      incoming.innerHTML = e.detail.ctx.text;
      highlightSelection(incoming);
      if (blocked) reconcileCapturedTasks(target, incoming, new Set(captures.values()));
      else reconcileTasks(target, incoming);
      for (const [key, id] of captures || []) {
        const selector = `.task[data-id="${CSS.escape(id)}"]`;
        if (
          blocked &&
          !target.querySelector(selector) &&
          incoming.querySelector(selector)
        )
          continue;
        pendingRows.get(key)?.remove();
        pendingRows.delete(key);
        confirmedCaptures.delete(key);
      }
      if (blocked) $('#task-count').textContent = $$('#task-list .task').length;
      else
        target.dispatchEvent(
          new CustomEvent('htmx:after:swap', {
            bubbles: true,
            detail: { ctx: { target } },
          }),
        );
    }
  });
  scope.listen(document.body, 'htmx:after:swap', (e) => {
    if (e.target.id === 'task-list')
      $('#task-count').textContent = $$('#task-list .task').length;
    if (
      e.target.id === 'task-list' &&
      restoreTaskFocus &&
      (document.activeElement === document.body ||
        $('#task-list').contains(document.activeElement))
    ) {
      const row = $$('#task-list .task').find(
        (r) => r.dataset.id === restoreTaskFocus.id,
      );
      row?.querySelector(restoreTaskFocus.selector)?.focus({ preventScroll: true });
      restoreTaskFocus = null;
    }
  });
  const $$ = (s) => [...main.querySelectorAll(s)];
  scope.listen(document.body, 'htmx:response:error', ({ detail: { ctx } }) => {
    // The capture queue retries the list read if the save already succeeded.
    if (!ctx?.todoerCapturedTasks?.size)
      notice('Could not refresh tasks. Try again.', true);
  });
  scope.listen(main, 'todoer:completed-filter', () => refresh());

  $('#quick-add').onclick = () => {
    $('#capture-form').hidden = false;
    $('#capture-title')?.focus();
  };
  scope.listen(document, 'keydown', (e) => {
    if (
      e.key === 'n' &&
      !e.ctrlKey &&
      !e.metaKey &&
      !e.altKey &&
      !e.target.closest('input,textarea,[contenteditable="true"]') &&
      !taskPanel.isOpen
    ) {
      e.preventDefault();
      $('#capture-form').hidden = false;
      $('#capture-title')?.focus();
    }
  });
  let replaying = false;
  let replayAgain = false;
  async function showPending() {
    const records = await listPending(account);
    if (scope.signal.aborted) return;
    const pendingKeys = new Set(confirmedCaptures.keys());
    for (const record of records) {
      if (confirmedCaptures.has(record.key)) continue;
      if (record.payload.list_id !== list || record.status === 'needs_attention')
        continue;
      const section = $$('#task-list .task-section').find(
        (section) => section.dataset.sectionId === (record.payload.section_id || ''),
      );
      if (!section) continue;
      pendingKeys.add(record.key);
      let row = pendingRows.get(record.key);
      if (!row) {
        const { title, description } = record.payload.tasks[0];
        row = pendingTaskRow(
          'article',
          title,
          description,
          record.payload.metadata?.priority,
        );
        pendingRows.set(record.key, row);
      }
      row.querySelector('.pending-task-state').textContent =
        !navigator.onLine || record.error ? 'Pending sync' : 'Saving…';
      if (!row.isConnected) section.querySelector('.section-tasks').append(row);
    }
    for (const [key, row] of pendingRows) {
      if (pendingKeys.has(key)) continue;
      row.remove();
      pendingRows.delete(key);
    }
    const container = $('#pending-captures');
    container.replaceChildren();
    for (const record of records) {
      if (confirmedCaptures.has(record.key)) continue;
      if (pendingRows.has(record.key) && navigator.onLine && !record.error) continue;
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
      await showPending();
      if (!navigator.onLine) return;
      const result = await replayCaptures(account, async (record) => {
        const [task] = await api('/capture', 'POST', record.payload, {
          'Idempotency-Key': record.key,
        });
        if (task?.list_id === list) {
          confirmedCaptures.set(record.key, task.id);
          captureRevisions.set(task.id, task.revision);
          if (captureRevisions.size > 1000)
            captureRevisions.delete(captureRevisions.keys().next().value);
        }
      });
      if (result.sent || confirmedCaptures.size) refresh({ quiet: true });
      if (result.errors.length && navigator.onLine)
        notice(result.errors[0].message, true);
      await showPending();
    } catch (err) {
      notice(`Local capture storage is unavailable: ${err.message}`, true);
    } finally {
      replaying = false;
      if (replayAgain && !scope.signal.aborted) {
        replayAgain = false;
        replay();
      }
    }
  }
  mountCaptures({
    account,
    listId: list,
    canWrite,
    notice,
    onQueued: async () => {
      if (replaying) {
        replayAgain = true;
        await showPending();
      } else await replay();
    },
    setBusy: (value) => {
      activeCaptures += value ? 1 : -1;
    },
    onSettled: resumeRefresh,
  });
  scope.listen(window, 'offline', replay);
  $('#retry-captures').onclick = replay;
  scope.listen(window, 'online', replay);
  scope.interval(() => {
    if (navigator.onLine) replay();
  }, 15000);
  replay();
  scope.listen($('#task-list'), 'click', async (e) => {
    const row = e.target.closest('.task');
    if (!row) return;
    if (row.hasAttribute('data-trash-pending')) return;
    if (e.target.closest('[data-trash]')) {
      if (!canWrite || row.hasAttribute('data-completion-pending')) return;
      row.dataset.trashPending = '';
      row.querySelector('details').open = false;
      try {
        if (
          await trashTask(
            api,
            { id: row.dataset.id, revision: Number(row.dataset.revision) },
            row,
            scope.signal,
          )
        ) {
          row.remove();
          $('#task-count').textContent = $$('#task-list .task').length;
          pendingRefresh = true;
        }
      } catch (error) {
        notice(error.message, true);
        pendingRefresh ||= error.status === 409;
      } finally {
        delete row.dataset.trashPending;
        if (pendingRefresh && !draftActive()) refresh();
      }
      return;
    }
    const forceSubtree = Boolean(e.target.closest('[data-action="complete-subtree"]'));
    if (e.target.closest('.completion') || forceSubtree) {
      const button = row.querySelector('.completion');
      if (button.disabled || !canWrite) return;
      if (forceSubtree) row.querySelector('details')?.removeAttribute('open');
      const wasComplete = row.dataset.complete === 'true';
      const complete = forceSubtree || !wasComplete;
      button.disabled = true;
      row.dataset.completionPending = '';
      const previous = button.innerHTML;
      let animation;
      let refreshAfter = false;
      try {
        const change = await taskCompletionChange(
          api,
          {
            id: row.dataset.id,
            revision: Number(row.dataset.revision),
            complete: wasComplete,
          },
          forceSubtree,
        );
        if (!change || scope.signal.aborted) return;
        row.classList.toggle('is-complete', complete);
        button.setAttribute('aria-pressed', String(complete));
        button.innerHTML = complete
          ? '<svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#check"/></svg>'
          : '';
        if (wasComplete !== complete) animation = animateTaskCompletion(button, complete);
        const task = await api(`/tasks/${row.dataset.id}`, 'PATCH', change);
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
        refreshAfter = change.complete_subtree;
      } catch (err) {
        row.classList.toggle('is-complete', wasComplete);
        button.setAttribute('aria-pressed', String(wasComplete));
        button.innerHTML = previous;
        notice(
          `${err.message} The previous display has been restored; refresh to confirm the saved state.`,
          true,
        );
        refreshAfter = err.status === 409;
      } finally {
        await animation;
        delete row.dataset.completionPending;
        button.disabled = false;
        if (refreshAfter) refresh();
        else resumeRefresh();
      }
    } else if (e.target.closest('[data-action="rename"]')) {
      row.querySelector('details')?.removeAttribute('open');
      await renameInline(row);
    } else if (e.target.closest('[data-action="edit"]')) {
      row.querySelector('details')?.removeAttribute('open');
      await openTask(row.dataset.id);
    } else if (
      !e.metaKey &&
      !e.ctrlKey &&
      !e.shiftKey &&
      !e.altKey &&
      !e.target.closest('button,input,textarea,select,summary,details,form') &&
      !window.getSelection()?.toString()
    ) {
      const link = e.target.closest('a');
      if (link) return;
      e.preventDefault();
      await openTask(row.dataset.id);
    }
  });
  scope.listen($('#task-list'), 'contextmenu', (event) => {
    const row = event.target.closest('.task');
    if (
      !canWrite ||
      !row ||
      event.target.closest('input,textarea,button,summary,details,a:not(.task-title)')
    )
      return;
    event.preventDefault();
    renameInline(row);
  });
  const isListChange = (change) =>
    change.task?.list_id === list || change.source_list_id === list;
  async function refreshLive(after, through, knownChanges) {
    if (activeCaptures || replaying) return refresh({ quiet: true });
    if (!through || !draftActive() || !captureRevisions.size) return refresh();
    let quiet = false;
    try {
      // The stream carries counts. Identify any late notification for our own
      // capture before showing a stale-list warning over an unrelated draft.
      const changes = knownChanges || (await api(`/changes?after=${after}`));
      const updates = changes.filter(
        (change) => change.seq <= through && isListChange(change),
      );
      quiet =
        updates.length > 0 &&
        updates.every(({ task }) => task?.revision <= captureRevisions.get(task?.id));
      for (const { task } of updates) captureRevisions.delete(task?.id);
    } catch {
      // An unrelated update may still need the usual draft protection.
    }
    refresh({ quiet });
  }
  if ('EventSource' in window) {
    const events = scope.track(
      new EventSource(
        `/api/v1/events?after=${cursor}&list_id=${encodeURIComponent(list)}`,
      ),
    );
    scope.listen(events, 'changes', (e) => {
      const data = JSON.parse(e.data);
      if (data.resync || (data.cursor > cursor && data.count > 0)) {
        const after = cursor;
        cursor = data.cursor || cursor;
        refreshLive(after, data.resync ? null : cursor);
      }
      cursor = Math.max(cursor, data.cursor || cursor);
      $('#connection').textContent = 'Up to date';
    });
    scope.listen(events, 'access-revoked', async () => {
      events.close();
      taskPanel.close();
      await clearPwa(account);
      $('#task-list').replaceChildren();
      document.querySelector('.list-description')?.remove();
      document.querySelector('.breadcrumbs').replaceChildren();
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
    scope.listen(events, 'session-expired', () => {
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
          const after = cursor;
          cursor = Math.max(cursor, ...changes.map((c) => c.seq));
          if (changes.some(isListChange)) refreshLive(after, cursor, changes);
        }
        $('#connection').textContent = 'Up to date';
      } catch {
        $('#connection').textContent = 'Reconnecting';
      }
      scope.timeout(poll, 3000);
    };
    poll();
  }
  // Preserve direct list task links while using the same task panel as Today.
  const taskId = new URLSearchParams(location.search).get('task');
  if (taskId) openTask(taskId);
  function openTask(id) {
    const link = $(`#task-list .task[data-id="${CSS.escape(id)}"] .task-title`);
    return taskPanel.open(
      link?.getAttribute('href') || `/tasks/${encodeURIComponent(id)}`,
    );
  }
  scope.listen(window, 'pagehide', () => taskPanel.destroy());

  const swipeTask = createTaskSwipeActions({
    canWrite,
    api,
    notice,
    changed: () => refresh(),
    getTask: (row) => ({
      id: row.dataset.id,
      parent_id: row.dataset.parentId,
      revision: Number(row.dataset.revision),
      complete: row.dataset.complete === 'true',
      title: row.querySelector('.task-title')?.textContent.trim(),
    }),
  });
  scope.cleanup(swipeTask.destroy);

  mountSectionOrdering({
    container: $('#task-list'),
    listId: list,
    canWrite,
    request: api,
    changed: () => refresh(),
    error: (message) => notice(message, true),
    blocked: () => taskPanel.isOpen || draftActive(true),
  });

  scope.cleanup(
    mountTaskOrdering({
      container: $('#task-list'),
      listId: list,
      canWrite,
      request: api,
      onChanged: () => refresh(),
      onError: (message) => notice(message, true),
      onSwipe: swipeTask,
      hasActiveDraft: () =>
        taskPanel.isOpen ||
        inlineEditors.size > 0 ||
        Boolean(document.querySelector('#capture-title')?.value) ||
        Boolean(window.getSelection()?.toString()),
    }),
  );
  async function renameInline(row, snapshot) {
    if (!canWrite) return;
    const id = row.dataset.id;
    if (inlineEditors.has(id)) return;
    inlineEditors.set(id, null);
    try {
      const task =
        snapshot || (await api(`/tasks/${id}/subtree`)).find((t) => t.id === id);
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
      form.className = 'inline-editor entry-form';
      const input = document.createElement('input');
      input.name = 'title';
      input.value = task.title;
      input.required = true;
      input.maxLength = 500;
      input.setAttribute('aria-label', 'Task title');
      const description = document.createElement('textarea');
      description.name = 'description';
      description.value = task.description || '';
      description.rows = 3;
      description.maxLength = 100000;
      description.placeholder = 'Description (Markdown supported)';
      description.setAttribute('aria-label', 'Task description');
      const previousDescription = row.querySelector('.task-description');
      if (previousDescription) previousDescription.hidden = true;
      const save = document.createElement('button');
      save.type = 'submit';
      save.className = 'primary';
      save.textContent = 'Save';
      const cancel = document.createElement('button');
      cancel.type = 'button';
      cancel.className = 'secondary';
      cancel.textContent = 'Cancel';
      const key = `todoer:${account}:inline:${id}`;
      const stash = () =>
        sessionStorage.setItem(
          key,
          JSON.stringify({ ...task, title: input.value, description: description.value }),
        );
      input.oninput = stash;
      description.oninput = stash;
      inlineEditors.set(id, form);
      display.hidden = true;
      form.append(input, description, save, cancel);
      display.before(form);
      const saveStructuredInline = mountStructuredEdit(input);
      const close = () => {
        sessionStorage.removeItem(key);
        form.remove();
        display.hidden = false;
        if (previousDescription) previousDescription.hidden = false;
        inlineEditors.delete(id);
        display.focus();
        resumeRefresh();
      };
      cancel.onclick = close;
      form.onkeydown = (e) => {
        if (e.key === 'Escape') {
          e.preventDefault();
          close();
        }
      };
      form.onsubmit = async (e) => {
        e.preventDefault();
        e.stopPropagation();
        save.disabled = true;
        input.disabled = true;
        description.disabled = true;
        cancel.disabled = true;
        form.setAttribute('aria-busy', 'true');
        try {
          const result = await saveStructuredInline(api, id, {
            revision: task.revision,
            title: input.value,
            description: description.value,
          });
          if (!result) return;
          display.textContent = result.title;
          row.dataset.revision = result.revision;
          row
            .querySelector('.subtask-edit')
            ?.setAttribute('aria-label', `Edit title for ${result.title}`);
          close();
          refresh({ force: true });
          notice('');
        } catch (err) {
          stash();
          notice(`${err.message}. Your inline draft is preserved.`, true);
        } finally {
          save.disabled = false;
          input.disabled = false;
          description.disabled = false;
          cancel.disabled = false;
          form.removeAttribute('aria-busy');
        }
      };
      input.focus();
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
});
