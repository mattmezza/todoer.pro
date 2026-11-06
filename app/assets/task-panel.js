import { beginLoading } from './loading.js';
import { request as fetch } from './request.js';
import { setSaveState } from './quiet-info.js';
import { mountTaskPage } from './task-page.js';

// One task surface for lists, Inbox and Today; task-page owns its editing logic.
export function createTaskPanel({
  csrf,
  onChanged = () => {},
  onSelected = () => {},
  onClose = () => {},
  dismissAfter,
} = {}) {
  const pagePath = location.pathname;
  let activeURL;
  let destroyed = false;
  let dismissing = false;
  const ownsHistory = () => history.state?.todoerTaskPanel?.page === pagePath;
  let panel,
    selected,
    generation = 0,
    loading = false,
    reading,
    stop = () => {},
    refreshPanelTags = () => {};
  const dismiss = document.createElement('button');
  dismiss.type = 'button';
  dismiss.className = 'planning-detail-dismiss';
  dismiss.setAttribute('aria-label', 'Close task details');
  dismiss.title = 'Close task details';
  dismiss.innerHTML =
    '<svg class="ui-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><rect x="3" y="3" width="18" height="18" rx="2"/><path d="M14 3v18"/></svg>';
  dismiss.onclick = close;
  function close(animate = true, updateHistory = true) {
    if (updateHistory && panel && ownsHistory()) {
      if (!dismissing) {
        dismissing = true;
        history.back();
      }
      return;
    }
    const wasOpen = Boolean(panel);
    activeURL = null;
    generation++;
    reading?.abort();
    stop();
    stop = () => {};
    refreshPanelTags = () => {};
    const closingPanel = panel;
    if (
      closingPanel &&
      animate &&
      !matchMedia('(prefers-reduced-motion: reduce)').matches
    ) {
      closingPanel.style.pointerEvents = 'none';
      closingPanel
        .animate([{ transform: 'translateX(0)' }, { transform: 'translateX(100%)' }], {
          duration: 260,
          easing: 'cubic-bezier(.22,.8,.25,1)',
          fill: 'forwards',
        })
        .finished.then(() => closingPanel.remove());
    } else closingPanel?.remove();
    panel = null;
    dismiss.remove();
    selected = null;
    document.body.classList.remove('planning-detail-open');
    if (wasOpen) {
      onClose();
      onSelected(selected);
    }
  }
  async function open(url, updateHistory = true) {
    if (destroyed) return;
    url = new URL(url, location.href).href;
    if (updateHistory) {
      const state = {
        ...history.state,
        htmx: true,
        todoerTaskPanel: { page: pagePath, url },
      };
      // Changing the preview replaces its single entry; Back always dismisses it.
      if (ownsHistory()) history.replaceState(state, '', location.href);
      else history.pushState(state, '', location.href);
    }
    activeURL = url;
    const version = ++generation;
    reading?.abort();
    reading = new AbortController();
    const signal = reading.signal;
    const read = (url, options = {}) => fetch(url, { ...options, signal });
    stop();
    stop = () => {};
    if (!panel) {
      panel = document.createElement('aside');
      panel.className = 'planning-detail-panel';
      panel.setAttribute('aria-label', 'Task details');
      document.body.append(panel);
    }
    panel.replaceChildren();
    document.body.classList.add('planning-detail-open');
    const compact = matchMedia('(max-width: 640px)').matches;
    if (!compact) {
      const anchor = dismissAfter?.();
      if (anchor) anchor.after(dismiss);
      else document.body.append(dismiss);
    }
    const header = document.createElement('header');
    header.className = 'planning-detail-toolbar';
    const content = document.createElement('div');
    content.className = 'planning-detail-content';
    const pending = beginLoading(content, { variant: 'spinner' });
    if (compact) header.append(dismiss);
    panel.append(header, content);
    try {
      const response = await read(url, { cache: 'no-store' });
      if (!response.ok) throw Error('Could not open this task.');
      const doc = new DOMParser().parseFromString(await response.text(), 'text/html');
      if (version !== generation) return;
      const task = doc.querySelector('[data-task-page]');
      if (!task) throw Error('Task is no longer accessible.');
      const openPage = document.createElement('a');
      openPage.href = `/tasks/${encodeURIComponent(task.dataset.taskId)}`;
      openPage.className = 'planning-detail-open-page';
      openPage.dataset.fullTask = '';
      openPage.setAttribute('aria-label', 'Open task page');
      openPage.title = 'Open task page';
      openPage.innerHTML =
        '<svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#open"/></svg>';
      const tools = document.createElement('div');
      tools.className = 'panel-heading-tools';
      header.replaceChildren(task.querySelector('[data-copy-task-reference]'), tools);
      if (compact) header.append(dismiss);
      task.prepend(header);
      selected = task.dataset.taskId;
      onSelected(selected);
      task.removeAttribute('id');
      task.dataset.embedded = 'true';
      let priorityVersion = 0;
      const reprio = document.createElement('div');
      reprio.className = 'planning-quick-actions planning-reprio';
      const picker = document.createElement('details');
      picker.className = 'panel-priority-picker';
      const priorityCaption = document.createElement('summary');
      priorityCaption.textContent = 'Priority';
      priorityCaption.setAttribute('aria-label', 'Change priority');
      picker.append(priorityCaption);
      const choices = document.createElement('div');
      choices.className = 'panel-priority-options';
      picker.append(choices);
      reprio.append(picker);
      tools.append(reprio, openPage, task.querySelector('.task-context-menu'));
      function selectPriority(priority) {
        task.dataset.priority = String(priority);
        const labels = ['Highest', 'High', 'Medium', 'Low', 'Lowest'];
        priorityCaption.textContent = `P${priority}`;
        priorityCaption.title = `${labels[priority]} priority`;
        priorityCaption.dataset.priority = priority;
        for (const b of choices.querySelectorAll('button'))
          b.setAttribute('aria-pressed', String(Number(b.dataset.priority) === priority));
      }
      for (let priority = 0; priority <= 4; priority++) {
        const button = document.createElement('button');
        button.type = 'button';
        button.textContent = `P${priority} · ${['Highest', 'High', 'Medium', 'Low', 'Lowest'][priority]} priority`;
        button.dataset.priority = priority;
        button.disabled = task.dataset.canWrite !== 'true';
        button.onclick = async () => {
          if (loading) return;
          priorityVersion++;
          loading = true;
          setSaveState('preview-priority', 'saving');
          for (const b of reprio.querySelectorAll('button')) b.disabled = true;
          try {
            const current = await fetch(
              `/api/v1/tasks/${encodeURIComponent(task.dataset.taskId)}/schedule`,
              { cache: 'no-store' },
            );
            if (!current.ok) throw Error('Could not load task priority.');
            const saved = await current.json();
            const response = await fetch(
              `/api/v1/tasks/${encodeURIComponent(task.dataset.taskId)}/schedule`,
              {
                method: 'PUT',
                headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': csrf },
                body: JSON.stringify({
                  revision: saved.task_revision,
                  schedule: { ...saved.schedule, priority },
                }),
              },
            );
            if (!response.ok)
              throw Error(
                (await response.json()).error?.message || 'Could not save priority.',
              );
            selectPriority(priority);
            picker.open = false;
            setSaveState('preview-priority', 'saved');
            await onChanged();
          } catch (error) {
            setSaveState('preview-priority', 'error', error.message);
          } finally {
            loading = false;
            for (const b of reprio.querySelectorAll('button'))
              b.disabled = task.dataset.canWrite !== 'true';
          }
        };
        choices.append(button);
      }
      if (task.dataset.canWrite !== 'true')
        priorityCaption.setAttribute('aria-disabled', 'true');
      // Mount behind one loading state until all visible sections have settled.
      content.append(task);
      stop = mountTaskPage(task, {
        embedded: true,
        onOpenTask: open,
        onSchedule: (value) => {
          if (version === generation && priorityVersion === 0)
            selectPriority(value.schedule.priority ?? 3);
        },
      });
      const tags = document.createElement('div');
      tags.className = 'panel-task-tags';
      tags.setAttribute('aria-label', 'Task tags');
      tags.hidden = true;
      task.querySelector('.task-page-brief').after(tags);
      let tagsVersion = 0;
      refreshPanelTags = async () => {
        const requestVersion = ++tagsVersion;
        try {
          const response = await read(
            `/api/v1/tasks/${encodeURIComponent(task.dataset.taskId)}/tags`,
            { cache: 'no-store' },
          );
          if (!response.ok) return;
          const savedTags = await response.json();
          if (version !== generation || requestVersion !== tagsVersion) return;
          tags.replaceChildren(
            ...savedTags.map((tag) => {
              const item = document.createElement('span');
              item.className = 'inbox-tag';
              item.textContent = `+${tag.name}`;
              return item;
            }),
          );
          tags.hidden = !savedTags.length;
        } catch {
          // Keep the last successfully loaded tags during a transient failure.
        }
      };
      await Promise.all([stop.ready, refreshPanelTags()]);
    } catch (error) {
      if (version === generation) pending.fail(error.message, () => open(url));
    } finally {
      pending.finish();
    }
  }

  function restoreHistory() {
    if (destroyed) return;
    dismissing = false;
    const saved = history.state?.todoerTaskPanel;
    if (location.pathname === pagePath && saved?.page === pagePath) {
      if (activeURL !== saved.url) open(saved.url, false);
    } else close(true, false);
  }
  function keydown(event) {
    if (
      !panel ||
      event.key !== 'Escape' ||
      event.defaultPrevented ||
      document.querySelector('dialog[open]') ||
      event.target.closest('input,textarea,select')
    )
      return;
    const picker = panel.querySelector('.panel-priority-picker[open]');
    if (picker) {
      event.preventDefault();
      picker.open = false;
      picker.querySelector('summary').focus();
      return;
    }
    close();
  }
  function trashed(event) {
    if (!panel?.contains(event.target)) return;
    close();
    onChanged();
  }
  window.addEventListener('popstate', restoreHistory);
  if (ownsHistory()) queueMicrotask(restoreHistory);
  document.addEventListener('keydown', keydown);
  document.addEventListener('todoer:task-trashed', trashed);
  return {
    open,
    close,
    get isOpen() {
      return Boolean(panel);
    },
    contains: (node) => Boolean(panel?.contains(node)),
    refreshTags: () => refreshPanelTags(),
    destroy() {
      destroyed = true;
      close(false, false);
      window.removeEventListener('popstate', restoreHistory);
      document.removeEventListener('keydown', keydown);
      document.removeEventListener('todoer:task-trashed', trashed);
    },
  };
}
