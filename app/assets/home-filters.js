const main = document.querySelector(
  '[data-planning-today]:not([data-workbench]):not([data-search-preview])',
);
if (main) {
  const rows = main.querySelector('[data-planning-rows]');
  const spaces = main.querySelector('.planning-space-filters');
  const bar = document.createElement('nav');
  bar.className = 'planning-priority-filters';
  bar.setAttribute('aria-label', 'Filter by priority');
  const selected = new Set();
  function readSelection() {
    selected.clear();
    for (const value of (new URLSearchParams(location.search).get('prio') || '').split(
      ',',
    ))
      if (/^[0-3]$/.test(value)) selected.add(value);
  }
  readSelection();
  let buttons = [];
  function apply() {
    for (const row of rows.querySelectorAll('[data-planning-task]')) {
      const priority = row.querySelector('[data-priority]')?.dataset.priority || '3';
      row.hidden = selected.size > 0 && !selected.has(priority);
    }
    for (const [value, b] of buttons)
      b.setAttribute(
        'aria-pressed',
        String(value === 'all' ? !selected.size : selected.has(value)),
      );
    for (const a of main.querySelectorAll(
      '.planning-tabs a,.planning-space-filters a,.planning-zone a',
    )) {
      const url = new URL(a.href, location.href);
      if (selected.size) url.searchParams.set('prio', [...selected].sort().join(','));
      else url.searchParams.delete('prio');
      a.href = url;
    }
    const form = main.querySelector('.planning-zone form');
    if (form) {
      let hidden = form.querySelector('[name=prio]');
      if (!hidden) {
        hidden = document.createElement('input');
        hidden.type = 'hidden';
        hidden.name = 'prio';
        form.append(hidden);
      }
      hidden.value = [...selected].sort().join(',');
    }
  }
  for (const [value, label] of [
    ['all', 'All priorities'],
    ['0', 'P0'],
    ['1', 'P1'],
    ['2', 'P2'],
    ['3', 'P3'],
  ]) {
    const b = document.createElement('button');
    b.type = 'button';
    b.textContent = label;
    b.dataset.priority = value;
    b.onclick = () => {
      if (value === 'all') selected.clear();
      else if (selected.has(value)) selected.delete(value);
      else selected.add(value);
      if (selected.size === 4) selected.clear();
      const url = new URL(location.href);
      if (selected.size) url.searchParams.set('prio', [...selected].sort().join(','));
      else url.searchParams.delete('prio');
      history.pushState(null, '', url);
      apply();
    };
    buttons.push([value, b]);
    bar.append(b);
  }
  const filters = document.createElement('div');
  filters.className = 'planning-secondary-filters';
  spaces.before(filters);
  filters.append(spaces, bar);
  main.querySelector('.planning-tabs')?.before(filters);
  const empty = document.createElement('p');
  empty.className = 'planning-priority-empty muted';
  empty.textContent = 'No tasks match these priorities.';
  empty.hidden = true;
  rows.after(empty);
  function refresh() {
    apply();
    const all = [...rows.querySelectorAll('[data-planning-task]')];
    empty.hidden = !all.length || all.some((row) => !row.hidden);
  }
  for (const [, b] of buttons) b.addEventListener('click', refresh);
  addEventListener('popstate', () => {
    readSelection();
    refresh();
  });
  main.addEventListener('todoer:planning-navigation', () => {
    readSelection();
    refresh();
  });
  new MutationObserver(refresh).observe(rows, { childList: true, subtree: true });
  refresh();
}
