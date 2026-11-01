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
    rowsGeneration = 0,
    filterLoading = false,
    pendingRowsRefresh = false,
    refreshPanelTags = () => {};
  const csrf = document.querySelector('meta[name=csrf-token]').content;
  const rows = main.querySelector('[data-planning-rows]');
  const dismiss = document.createElement('button');
  dismiss.type = 'button';
  dismiss.className = 'planning-detail-dismiss';
  dismiss.setAttribute('aria-label', 'Close task details');
  dismiss.title = 'Close task details';
  dismiss.innerHTML =
    '<svg class="ui-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><rect x="3" y="3" width="18" height="18" rx="2"/><path d="M14 3v18"/></svg>';
  dismiss.onclick = close;
  async function refreshRows() {
    if (filterLoading) {
      pendingRowsRefresh = true;
      return;
    }
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
  function filterKey(url) {
    const value = new URL(url, location.href);
    value.searchParams.delete('prio');
    value.searchParams.sort();
    return value.pathname + value.search;
  }
  let loadedFilterKey = filterKey(location.href);
  // Keep the page and the open task panel mounted while changing Today filters.
  async function navigateFilters(url, push = true) {
    const version = ++rowsGeneration;
    filterLoading = true;
    main.querySelector('.planning-filter-error')?.remove();
    const skeleton = beginLoading(rows, { variant: 'planning-list' });
    try {
      const response = await fetch(url, {
        cache: 'no-store',
        headers: { 'X-Todoer-Planning-Fragment': '1' },
      });
      if (!response.ok) throw Error('Could not load planned tasks.');
      const doc = new DOMParser().parseFromString(await response.text(), 'text/html');
      const next = doc.querySelector('[data-planning-rows]');
      if (!next)
        throw Error('Could not load planned tasks. Sign in again if your session ended.');
      if (version !== rowsGeneration) return;
      if (push) {
        const destination = new URL(url, location.href);
        const priority = new URLSearchParams(location.search).get('prio');
        if (priority) destination.searchParams.set('prio', priority);
        else destination.searchParams.delete('prio');
        history.pushState(null, '', destination);
      }
      for (const selector of ['.planning-tabs', '.planning-space-filters']) {
        const current = main.querySelector(selector);
        const incoming = doc.querySelector(selector);
        if (!current || !incoming) continue;
        const left = current.scrollLeft;
        current.replaceChildren(...incoming.childNodes);
        current.scrollLeft = left;
      }
      for (const selector of ['.planning-welcome h1', '.planning-zone summary']) {
        const current = main.querySelector(selector),
          incoming = doc.querySelector(selector);
        if (current && incoming) current.textContent = incoming.textContent;
      }
      for (const name of ['range', 'spaces', 'tz']) {
        const current = main.querySelector(`.planning-zone form [name="${name}"]`);
        const incoming = doc.querySelector(`.planning-zone form [name="${name}"]`);
        if (current && incoming) current.value = incoming.value;
      }
      const accountZone = main.querySelector('.planning-zone form a');
      const incomingZone = doc.querySelector('.planning-zone form a');
      if (accountZone && incomingZone) accountZone.href = incomingZone.href;
      main.dataset.planningToday = doc.querySelector(
        '[data-planning-today]',
      ).dataset.planningToday;
      skeleton.finish();
      rows.replaceChildren(...next.childNodes);
      loadedFilterKey = filterKey(location.href);
      main.dispatchEvent(new Event('todoer:planning-navigation'));
      highlight();
    } catch (error) {
      if (version === rowsGeneration) {
        skeleton.finish();
        const notice = document.createElement('div');
        notice.className = 'planning-filter-error';
        notice.setAttribute('role', 'alert');
        const message = document.createElement('span');
        message.textContent = error.message;
        const retry = document.createElement('button');
        retry.type = 'button';
        retry.textContent = 'Try again';
        retry.onclick = () => navigateFilters(url, push);
        notice.append(message, retry);
        rows.before(notice);
      }
    } finally {
      if (version === rowsGeneration) {
        filterLoading = false;
        skeleton.finish();
        if (pendingRowsRefresh) {
          pendingRowsRefresh = false;
          refreshRows();
        }
      }
    }
  }
  if (!main.dataset.workbench && !main.hasAttribute('data-search-preview')) {
    main.addEventListener('click', (event) => {
      const link = event.target.closest(
        '.planning-tabs a, .planning-space-filters a, .planning-zone a',
      );
      if (
        !link ||
        event.button !== 0 ||
        event.metaKey ||
        event.ctrlKey ||
        event.shiftKey ||
        event.altKey
      )
        return;
      event.preventDefault();
      navigateFilters(link.href);
    });
    main.querySelector('.planning-zone form')?.addEventListener('submit', (event) => {
      event.preventDefault();
      const url = new URL('/today', location.origin);
      for (const [name, value] of new FormData(event.currentTarget))
        if (value) url.searchParams.set(name, value);
      main.querySelector('.planning-zone').open = false;
      navigateFilters(url.href);
    });
    addEventListener('popstate', () => {
      if (filterKey(location.href) !== loadedFilterKey || filterLoading)
        navigateFilters(location.href, false);
    });
  }
  function highlight() {
    for (const row of rows.querySelectorAll('[data-planning-task]'))
      row.classList.toggle('is-selected', row.dataset.planningTask === selected);
  }
  function close() {
    generation++;
    stop();
    stop = () => {};
    refreshPanelTags = () => {};
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
    dismiss.remove();
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
    const exportButton = main.querySelector('[data-export-workbench]');
    if (exportButton) exportButton.after(dismiss);
    else document.body.append(dismiss);
    const header = document.createElement('header');
    header.className = 'planning-detail-toolbar';
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
      header.append(task.querySelector('[data-copy-task-reference]'), tools);
      task.prepend(header);
      selected = task.dataset.taskId;
      highlight();
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
        const labels = ['Highest', 'High', 'Medium', 'Low'];
        priorityCaption.textContent = `P${priority}`;
        priorityCaption.title = `${labels[priority]} priority`;
        priorityCaption.dataset.priority = priority;
        for (const b of choices.querySelectorAll('button'))
          b.setAttribute('aria-pressed', String(Number(b.dataset.priority) === priority));
      }
      for (let priority = 0; priority <= 3; priority++) {
        const button = document.createElement('button');
        button.type = 'button';
        button.textContent = `P${priority} · ${['Highest', 'High', 'Medium', 'Low'][priority]} priority`;
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
            await refreshRows();
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
      content.replaceChildren(task);
      stop = mountTaskPage(task, { embedded: true });
      const tags = document.createElement('div');
      tags.className = 'panel-task-tags';
      tags.setAttribute('aria-label', 'Task tags');
      tags.hidden = true;
      task.querySelector('.task-page-brief').after(tags);
      let tagsVersion = 0;
      refreshPanelTags = async () => {
        const requestVersion = ++tagsVersion;
        try {
          const response = await fetch(
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
      refreshPanelTags();
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
            selectPriority(value.schedule.priority ?? 3);
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
      const row = event.target.closest(
        '[data-planning-rows] :is(.inbox-task, .planning-task)',
      );
      link = row?.querySelector('a[href^="/tasks/"]');
    }
    if (link?.hasAttribute('data-full-task')) return;
    if (!link || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    if (matchMedia('(max-width: 640px)').matches) {
      if (!event.target.closest('a')) location.assign(link.href);
      return;
    }
    event.preventDefault();
    open(link.href);
  });
  document.addEventListener('keydown', (event) => {
    if (
      event.key !== 'Escape' ||
      event.defaultPrevented ||
      document.querySelector('dialog[open]') ||
      event.target.closest('input,textarea,select')
    )
      return;
    const picker = panel?.querySelector('.panel-priority-picker[open]');
    if (picker) {
      event.preventDefault();
      picker.open = false;
      picker.querySelector('summary').focus();
      return;
    }
    close();
  });
  let timer;
  const events = new EventSource('/api/v1/events');
  events.addEventListener('changes', (event) => {
    try {
      const change = JSON.parse(event.data);
      if (change.resync || change.count > 0) {
        clearTimeout(timer);
        timer = setTimeout(() => {
          refreshRows();
          refreshPanelTags();
        }, 250);
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

const selectedDate = document.querySelector('.planning-tabs a[aria-current="page"]');
if (selectedDate) {
  const dates = selectedDate.parentElement;
  dates.scrollLeft =
    selectedDate.offsetLeft -
    dates.offsetLeft -
    (dates.clientWidth - selectedDate.offsetWidth) / 2;
}
