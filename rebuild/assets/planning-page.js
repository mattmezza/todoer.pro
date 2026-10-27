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
    const version = ++rowsGeneration;
    try {
      const r = await fetch(location.href, { cache: 'no-store' });
      if (!r.ok) return;
      const doc = new DOMParser().parseFromString(await r.text(), 'text/html');
      if (version !== rowsGeneration) return;
      const next = doc.querySelector('[data-planning-rows]');
      if (next) rows.replaceChildren(...next.childNodes);
      highlight();
    } catch {}
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
      closingPanel.animate([{ transform: 'translateX(0)' }, { transform: 'translateX(100%)' }], { duration: 260, easing: 'cubic-bezier(.22,.8,.25,1)', fill: 'forwards' }).finished.then(() => closingPanel.remove());
    } else closingPanel?.remove();
    panel = null;
    selected = null;
    document.body.classList.remove('planning-detail-open');
    highlight();
  }
  const date = (offset) => {
    const day = new Date(main.dataset.planningToday + 'T12:00:00Z');
    day.setUTCDate(day.getUTCDate() + offset);
    return day.toISOString().slice(0, 10);
  };
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
    const title = document.createElement('strong');
    title.textContent = 'Task details';
    const full = document.createElement('a');
    full.href = url;
    full.textContent = 'Open full page';
    const dismiss = document.createElement('button');
    dismiss.type = 'button';
    dismiss.setAttribute('aria-label', 'Close task details');
    dismiss.innerHTML =
      '<svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#close"/></svg>';
    dismiss.onclick = close;
    header.append(title, full, dismiss);
    const content = document.createElement('div');
    content.className = 'planning-detail-content';
    content.textContent = 'Loading task…';
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
      const quick = document.createElement('div');
      quick.className = 'planning-quick-actions';
      const caption = document.createElement('span');
      caption.textContent = 'Replan';
      quick.append(caption);
      const weekday =
        new Date(main.dataset.planningToday + 'T12:00:00Z').getUTCDay() || 7;
      const nextWeek = 7 - ((weekday - Number(main.dataset.weekStart) + 7) % 7);
      const notice = document.createElement('p');
      notice.className = 'planning-quick-status';
      notice.role = 'status';
      for (const [label, offset] of [
        ['Today', 0],
        ['Tomorrow', 1],
        ['Next week', nextWeek],
      ]) {
        const button = document.createElement('button');
        button.type = 'button';
        button.textContent = label;
        button.disabled = task.dataset.canWrite !== 'true';
        button.onclick = async () => {
          if (loading) return;
          loading = true;
          for (const b of quick.querySelectorAll('button')) b.disabled = true;
          notice.textContent = 'Saving…';
          try {
            const saved = await fetch(
              `/api/v1/tasks/${encodeURIComponent(task.dataset.taskId)}/schedule`,
              { cache: 'no-store' },
            );
            if (!saved.ok) throw Error('Could not load the saved planning.');
            const state = await saved.json();
            const r = await fetch(
              `/api/v1/tasks/${encodeURIComponent(task.dataset.taskId)}/schedule`,
              {
                method: 'PUT',
                headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': csrf },
                body: JSON.stringify({
                  revision: state.task_revision,
                  schedule: {
                    ...state.schedule,
                    planned: { kind: 'date', date: date(offset) },
                  },
                }),
              },
            );
            if (!r.ok)
              throw Error(
                (await r.json()).error?.message || 'Planning was not saved. Try again.',
              );
            notice.textContent = `Planned for ${date(offset)}.`;
            await refreshRows();
          } catch (error) {
            notice.textContent = error.message;
          } finally {
            loading = false;
            for (const b of quick.querySelectorAll('button')) b.disabled = false;
          }
        };
        quick.append(button);
      }
      content.replaceChildren(quick, notice, task);
      stop = mountTaskPage(task, { embedded: true });
    } catch (error) {
      if (version === generation) content.textContent = error.message;
    }
  }
  document.addEventListener('click', (event) => {
    const link = event.target.closest(
      '[data-planning-rows] a[href^="/tasks/"],.planning-detail-content a[href^="/tasks/"]',
    );
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
