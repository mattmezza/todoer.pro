import { navigate } from './page-navigation.js';
import { beginLoading } from './loading.js';

import { setSaveState } from './quiet-info.js';
import { createTaskPanel } from './task-panel.js';
import { onPageLoad } from './page-lifecycle.js';

onPageLoad((scope) => {
  if (!document.querySelector('[data-planning-today]')) return;
  const fetch = scope.request;
  const main = document.querySelector('[data-planning-today]');
  if (main) {
    let selected,
      rowsGeneration = 0,
      filterLoading = false,
      pendingRowsRefresh = false;
    const csrf = document.querySelector('meta[name=csrf-token]').content;
    const rows = main.querySelector('[data-planning-rows]');
    const taskPanel = createTaskPanel({
      csrf,
      onChanged: refreshRows,
      dismissAfter: () => main.querySelector('[data-export-workbench]'),
      onSelected: (id) => {
        selected = id;
        highlight();
      },
    });
    scope.cleanup(() => taskPanel.destroy());
    async function refreshRows() {
      if (scope.signal.aborted) return;
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
          throw Error(
            'Could not load planned tasks. Sign in again if your session ended.',
          );
        if (version !== rowsGeneration) return;
        if (push) {
          const destination = new URL(url, location.href);
          const priority = new URLSearchParams(location.search).get('prio');
          if (priority) destination.searchParams.set('prio', priority);
          else destination.searchParams.delete('prio');
          history.pushState({ htmx: true }, '', destination);
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
      scope.listen(main, 'click', (event) => {
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
      scope.listen(main.querySelector('.planning-zone form'), 'submit', (event) => {
        event.preventDefault();
        const url = new URL('/today', location.origin);
        for (const [name, value] of new FormData(event.currentTarget))
          if (value) url.searchParams.set(name, value);
        main.querySelector('.planning-zone').open = false;
        navigateFilters(url.href);
      });
      scope.listen(window, 'popstate', () => {
        if (location.pathname !== '/today' && location.pathname !== '/upcoming') return;
        if (filterKey(location.href) !== loadedFilterKey || filterLoading)
          navigateFilters(location.href, false);
      });
    }
    function highlight() {
      for (const row of rows.querySelectorAll('[data-planning-task]'))
        row.classList.toggle('is-selected', row.dataset.planningTask === selected);
    }
    scope.listen(document, 'click', (event) => {
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
      if (!link || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey)
        return;
      if (matchMedia('(max-width: 640px)').matches) {
        if (!event.target.closest('a')) navigate(link.href);
        return;
      }
      event.preventDefault();
      taskPanel.open(link.href);
    });
    let timer;
    const events = scope.track(new EventSource('/api/v1/events'));
    scope.listen(events, 'changes', (event) => {
      try {
        const change = JSON.parse(event.data);
        if (change.resync || change.count > 0) {
          clearTimeout(timer);
          timer = scope.timeout(() => {
            refreshRows();
            taskPanel.refreshTags();
          }, 250);
        }
      } catch {}
    });
    scope.listen(window, 'pagehide', () => {
      events.close();
      clearTimeout(timer);
      taskPanel.destroy();
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
    scope.listen(document, 'click', (event) => {
      if (!picker.contains(event.target)) picker.open = false;
    });
    scope.listen(picker, 'keydown', (event) => {
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
});
