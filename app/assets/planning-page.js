import { beginLoading } from './loading.js';
import { request as fetch } from './request.js';
import { setSaveState } from './quiet-info.js';
import { mountTaskPage } from './task-page.js';
const main = document.querySelector('[data-planning-today]');
if (main) {
  let panel,
    stop = () => {},
    selected,
    generation = 0,
    loading = false,
    rowsGeneration = 0;
  const csrf = document.querySelector('meta[name=csrf-token]').content;
  const rows = main.querySelector('[data-planning-rows]');
  async function refreshRows() {
    if (main.dataset.workbench) {
      document.dispatchEvent(new Event('todoer:workbench-refresh'));
      return;
    }
    const version = ++rowsGeneration;
    try {
      const r = await fetch(location.href, { cache: 'no-store' });
      if (!r.ok)
        throw Error('Could not refresh tasks. Reload to see the latest saved state.');
      const doc = new DOMParser().parseFromString(await r.text(), 'text/html');
      if (version !== rowsGeneration) return;
      const next = doc.querySelector('[data-planning-rows]');
      if (!next)
        throw Error('Could not refresh tasks. Sign in again if your session ended.');
      rows.replaceChildren(...next.childNodes);
      highlight();
    } catch (error) {
      if (version === rowsGeneration)
        setSaveState('preview-refresh', 'error', error.message);
    }
  }
  function highlight() {
    for (const row of rows.querySelectorAll('[data-planning-task]'))
      row.classList.toggle('is-selected', row.dataset.planningTask === selected);
  }
  function close() {
    generation++;
    stop();
    stop = () => {};
    const closingPanel = panel;
    if (closingPanel && !matchMedia('(prefers-reduced-motion: reduce)').matches) {
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
    selected = null;
    document.body.classList.remove('planning-detail-open');
    highlight();
  }
  async function open(url) {
    const version = ++generation;
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
    const header = document.createElement('header');
    header.className = 'planning-detail-toolbar';
    const dismiss = document.createElement('button');
    dismiss.type = 'button';
    dismiss.setAttribute('aria-label', 'Close task details');
    dismiss.innerHTML =
      '<svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#close"/></svg>';
    dismiss.onclick = close;
    header.append(dismiss);
    const content = document.createElement('div');
    content.className = 'planning-detail-content';
    const skeleton = beginLoading(content, { variant: 'task' });
    panel.append(header, content);
    try {
      const response = await fetch(url, { cache: 'no-store' });
      if (!response.ok) throw Error('Could not open this task.');
      const doc = new DOMParser().parseFromString(await response.text(), 'text/html');
      if (version !== generation) return;
      const task = doc.querySelector('[data-task-page]');
      if (!task) throw Error('Task is no longer accessible.');
      selected = task.dataset.taskId;
      highlight();
      task.removeAttribute('id');
      task.dataset.embedded = 'true';
      let priorityVersion = 0;
      const reprio = document.createElement('div');
      reprio.className = 'planning-quick-actions planning-reprio';
      const priorityCaption = document.createElement('span');
      priorityCaption.textContent = 'Reprio';
      reprio.append(priorityCaption);
      for (let priority = 0; priority <= 3; priority++) {
        const button = document.createElement('button');
        button.type = 'button';
        button.textContent = 'P' + priority;
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
            for (const b of reprio.querySelectorAll('button'))
              b.setAttribute(
                'aria-pressed',
                String(Number(b.dataset.priority) === priority),
              );
            setSaveState('preview-priority', 'saved');
            await refreshRows();
          } catch (error) {
            setSaveState('preview-priority', 'error', error.message);
          } finally {
            loading = false;
            for (const b of reprio.querySelectorAll('button'))
              b.disabled = task.dataset.canWrite !== 'true';
          }
        };
        reprio.append(button);
      }
      content.replaceChildren(reprio, task);
      stop = mountTaskPage(task, { embedded: true });
      const initialPriorityVersion = priorityVersion;
      fetch(`/api/v1/tasks/${encodeURIComponent(task.dataset.taskId)}/schedule`, {
        cache: 'no-store',
      })
        .then((r) => (r.ok ? r.json() : null))
        .then((value) => {
          if (
            value &&
            version === generation &&
            priorityVersion === initialPriorityVersion
          )
            for (const b of reprio.querySelectorAll('button'))
              b.setAttribute(
                'aria-pressed',
                String(Number(b.dataset.priority) === value.schedule.priority),
              );
        })
        .catch(() => {});
    } catch (error) {
      if (version === generation) skeleton.fail(error.message, () => open(url));
    } finally {
      skeleton.finish();
    }
  }
  document.addEventListener('click', (event) => {
    let link = event.target.closest(
      '[data-planning-rows] a[href^="/tasks/"],.planning-detail-content a[href^="/tasks/"]',
    );
    if (
      !link &&
      !event.target.closest('button, input, select, textarea, a, summary, details')
    ) {
      const row = event.target.closest('[data-planning-rows] .inbox-task');
      link = row?.querySelector('a[href^="/tasks/"]');
    }
    if (link?.hasAttribute('data-full-task')) return;
    if (!link || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    open(link.href);
  });
  document.addEventListener('keydown', (event) => {
    if (
      event.key === 'Escape' &&
      !event.defaultPrevented &&
      !document.querySelector('dialog[open]') &&
      !event.target.closest('input,textarea,select')
    )
      close();
  });
  let timer;
  const events = new EventSource('/api/v1/events');
  events.addEventListener('changes', (event) => {
    try {
      const change = JSON.parse(event.data);
      if (change.resync || change.count > 0) {
        clearTimeout(timer);
        timer = setTimeout(refreshRows, 250);
      }
    } catch {}
  });
  addEventListener('pagehide', () => {
    events.close();
    clearTimeout(timer);
    close();
  });
}

// A URL-scoped override keeps account preferences unchanged.
const zones = document.querySelector('#planning-zones');
if (zones) {
  for (const zone of ['UTC', ...(Intl.supportedValuesOf?.('timeZone') || [])]) {
    const option = document.createElement('option');
    option.value = zone;
    zones.append(option);
  }
  const picker = document.querySelector('.planning-zone');
  document.addEventListener('click', (event) => {
    if (!picker.contains(event.target)) picker.open = false;
  });
  picker.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') picker.open = false;
  });
}

const selectedSpace = document.querySelector(
  '.planning-space-filters a[aria-current="true"]:not(:first-child)',
);
if (selectedSpace) selectedSpace.scrollIntoView({ block: 'nearest', inline: 'center' });
