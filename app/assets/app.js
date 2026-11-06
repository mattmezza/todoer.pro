import { createTaskPanel } from './task-panel.js';
import { mountStructuredEdit } from './structured-edit.js';
import { reconcileTasks } from './task-list-refresh.js';

import { stowInfo } from './quiet-info.js';
import { confirmAction } from './confirm.js';
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
  const taskPanel = createTaskPanel({
    csrf,
    onChanged: () => refresh(true),
    onSelected: (id) => {
      for (const row of $$('#task-list .task'))
        row.classList.toggle('is-selected', row.dataset.id === id);
    },
    onClose: () => {
      if (pendingRefresh) refresh();
    },
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
          if (pendingRefresh && !draftActive()) refresh();
        }, 0);
      },
      true,
    );
  let cursor = Number(document.body.dataset.cursor) || 0;
  let busy = false;
  let pendingRefresh = false;
  const inlineEditors = new Map();
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
      Boolean($('#task-list .task-actions[open]')) ||
      taskPanel.isOpen ||
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
    if (scope.signal.aborted) return;
    if (inlineEditors.size || taskPanel.isOpen || (!force && draftActive())) {
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
    if (taskPanel.isOpen || inlineEditors.size)
      notice('Finish or close the task editor first.');
    else refresh(true);
  };
  scope.listen(document.body, 'htmx:before:swap', (e) => {
    if (e.detail.ctx?.swap === 'none') return;
    const target = e.detail.ctx?.target;
    if (target?.id === 'task-list' && draftActive()) {
      e.preventDefault();
      pendingRefresh = true;
      $('#refresh-notice').hidden = false;
    } else if (target?.id === 'task-list') {
      const incoming = document.createElement('div');
      incoming.innerHTML = e.detail.ctx.text;
      reconcileTasks(target, incoming);
      e.preventDefault();
      target.dispatchEvent(
        new CustomEvent('htmx:after:swap', {
          bubbles: true,
          detail: { ctx: { target } },
        }),
      );
      $('#task-count').textContent = document.querySelectorAll('#task-list .task').length;
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
  scope.listen(document.body, 'htmx:response:error', () =>
    notice('Could not refresh tasks. Try again.', true),
  );
  // One preference per account, shared by every list in this browser.
  const completedPreferenceKey = `todoer:${account}:hide-completed`;
  const completedFilter = $('#show-completed');
  const completedCookieAccount = [...new TextEncoder().encode(account)]
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('');
  function syncCompletedCookie(hidden) {
    document.cookie = `todoer_completed=${completedCookieAccount}.${hidden ? '1' : '0'}; Path=/; Max-Age=31536000; SameSite=Lax${location.protocol === 'https:' ? '; Secure' : ''}`;
  }
  function applyCompletedPreference(hidden) {
    completedFilter.checked = !hidden;
    $('#task-list').classList.toggle('hide-completed', hidden);
  }
  let initiallyHidden = false;
  try {
    initiallyHidden = localStorage.getItem(completedPreferenceKey) === 'true';
    applyCompletedPreference(initiallyHidden);
  } catch {
    applyCompletedPreference(false);
  }
  syncCompletedCookie(initiallyHidden);
  if ($('#task-list').dataset.hideCompleted === 'true' && !initiallyHidden) refresh();
  completedFilter.onchange = () => {
    const hidden = !completedFilter.checked;
    applyCompletedPreference(hidden);
    syncCompletedCookie(hidden);
    try {
      localStorage.setItem(completedPreferenceKey, String(hidden));
    } catch {
      // Keep filtering usable when browser storage is unavailable.
    }
    refresh();
  };
  scope.listen(window, 'storage', (event) => {
    if (event.key !== completedPreferenceKey && event.key !== null) return;
    applyCompletedPreference(event.newValue === 'true');
    completedFilter.dispatchEvent(new Event('change'));
  });

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
      if (result.errors.length && navigator.onLine)
        notice(result.errors[0].message, true);
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
  scope.listen(window, 'online', replay);
  scope.interval(() => {
    if (navigator.onLine) replay();
  }, 15000);
  replay();
  scope.listen($('#task-list'), 'click', async (e) => {
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
    } else if (
      !e.metaKey &&
      !e.ctrlKey &&
      !e.shiftKey &&
      !e.altKey &&
      !e.target.closest('button,input,textarea,select,summary,details,form') &&
      !window.getSelection()?.toString()
    ) {
      const link = e.target.closest('a');
      if (link && !link.matches('.task-title')) return;
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
  if ('EventSource' in window) {
    const events = scope.track(
      new EventSource(
        `/api/v1/events?after=${cursor}&list_id=${encodeURIComponent(list)}`,
      ),
    );
    scope.listen(events, 'changes', (e) => {
      const data = JSON.parse(e.data);
      if (data.resync || (data.cursor > cursor && data.count > 0)) {
        taskPanel.refreshTags();
        cursor = data.cursor || cursor;
        refresh();
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
          cursor = Math.max(cursor, ...changes.map((c) => c.seq));
          if (changes.some((c) => c.task?.list_id === list || c.source_list_id === list))
            refresh();
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
    return taskPanel.open(`/tasks/${encodeURIComponent(id)}`);
  }
  scope.listen(window, 'pagehide', () => taskPanel.destroy());

  const swipeUndo = [];
  const swipingTasks = new Set();
  function showSwipeUndo() {
    for (const item of swipeUndo) item.element.remove();
    const latest = swipeUndo.at(-1);
    if (latest) document.body.append(latest.element);
  }
  async function swipeTask(row, action) {
    const id = row.dataset.id;
    if (
      !canWrite ||
      swipingTasks.has(id) ||
      (action === 'complete' && row.dataset.complete === 'true')
    )
      return;
    swipingTasks.add(id);
    row.setAttribute('aria-busy', 'true');
    try {
      const title = row.querySelector('.task-title')?.textContent.trim() || 'Task';
      const result = await api(
        action === 'archive'
          ? `/cli/lifecycle/task/${encodeURIComponent(id)}`
          : `/tasks/${encodeURIComponent(id)}`,
        action === 'archive' ? 'POST' : 'PATCH',
        action === 'archive'
          ? { archive: true }
          : { revision: Number(row.dataset.revision), complete: true },
      );
      const element = document.createElement('div');
      element.className = 'inbox-undo';
      element.setAttribute('role', 'status');
      const text = document.createElement('span');
      text.textContent = `${action === 'archive' ? 'Archived' : 'Completed'} · ${result.title || title}`;
      const undo = document.createElement('button');
      undo.type = 'button';
      undo.textContent = 'Undo';
      const item = { element };
      undo.onclick = async () => {
        undo.disabled = true;
        try {
          await api(
            action === 'archive'
              ? `/cli/lifecycle/task/${encodeURIComponent(id)}`
              : `/tasks/${encodeURIComponent(id)}`,
            action === 'archive' ? 'POST' : 'PATCH',
            action === 'archive'
              ? { archive: false }
              : { revision: result.revision, complete: false },
          );
          swipeUndo.splice(swipeUndo.indexOf(item), 1);
          element.remove();
          showSwipeUndo();
          refresh();
        } catch (error) {
          notice(`${error.message} The task could not be restored.`, true);
          undo.disabled = false;
        }
      };
      element.append(text, undo);
      swipeUndo.push(item);
      showSwipeUndo();
      refresh();
    } catch (error) {
      notice(`${error.message} The task was not changed.`, true);
      if (error.status === 409 || error.status === 404 || error.status === 403) refresh();
    } finally {
      row.removeAttribute('aria-busy');
      swipingTasks.delete(id);
    }
  }

  mountSectionOrdering({
    container: $('#task-list'),
    listId: list,
    canWrite,
    request: api,
    changed: () => refresh(),
    error: (message) => notice(message, true),
    blocked: () => draftActive(true),
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
      form.className = 'inline-editor';
      const input = document.createElement('input');
      input.value = task.title;
      input.required = true;
      input.maxLength = 500;
      input.setAttribute('aria-label', 'Task title');
      const description = document.createElement('textarea');
      description.value = task.description || '';
      description.rows = 3;
      description.maxLength = 100000;
      description.placeholder = 'Description (Markdown supported)';
      description.setAttribute('aria-label', 'Task description');
      const previousDescription = row.querySelector('.task-description');
      if (previousDescription) previousDescription.hidden = true;
      const save = document.createElement('button');
      save.type = 'submit';
      save.textContent = 'Save';
      const cancel = document.createElement('button');
      cancel.type = 'button';
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
        if (pendingRefresh) refresh();
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
          refresh(true);
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
