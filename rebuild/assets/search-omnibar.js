// Global quick search uses the same live, authorized ranked search contract.
let dialog, input, results, status, overview, controller, timer;
let generation = 0,
  active = -1;
function select(index) {
  const options = [...results.children];
  active = options.length ? (index + options.length) % options.length : -1;
  for (let i = 0; i < options.length; i++)
    options[i].setAttribute('aria-selected', String(i === active));
  if (active >= 0) {
    input.setAttribute('aria-activedescendant', options[active].id);
    options[active].scrollIntoView({ block: 'nearest' });
  } else input.removeAttribute('aria-activedescendant');
}
function destination(hit) {
  const id = encodeURIComponent(hit.task_id || hit.id);
  if (hit.task_id || hit.kind === 'task') return `/tasks/${id}`;
  if (hit.kind === 'workspace') return `/workspaces/${id}`;
  if (hit.kind === 'list') return `/lists/${id}`;
  if (hit.kind === 'section')
    return `/lists/${encodeURIComponent(hit.list_id)}?section=${id}`;
  if (hit.kind === 'tag') return `/search?tag=${id}`;
  return overview.href;
}
async function search() {
  const text = input.value.trim();
  const current = ++generation;
  controller?.abort();
  results.replaceChildren();
  select(-1);
  overview.href = `/search?query=${encodeURIComponent(text)}&status=all`;
  if (!text) {
    status.textContent = 'Search tasks, spaces, lists, comments, tags and files.';
    return;
  }
  status.textContent = 'Searching…';
  controller = new AbortController();
  try {
    const response = await fetch(
      `/api/v1/query?text=${encodeURIComponent(text)}&status=all&limit=8&sort=relevance`,
      { signal: controller.signal, cache: 'no-store' },
    );
    if (!response.ok)
      throw Error(
        response.status === 401
          ? 'Your session ended. Sign in again.'
          : 'Search is unavailable. Try again.',
      );
    const data = await response.json();
    if (current !== generation || !dialog.open) return;
    for (const [i, hit] of data.hits.entries()) {
      const row = document.createElement('a');
      row.href = destination(hit);
      row.id = `omnibar-result-${i}`;
      row.role = 'option';
      row.tabIndex = -1;
      const title = document.createElement('strong');
      title.textContent = hit.title || hit.kind;
      const context = document.createElement('small');
      context.textContent = [
        hit.kind,
        hit.complete ? 'completed' : '',
        ...(hit.context || []).map((c) => c.title),
      ]
        .filter(Boolean)
        .join(' · ');
      row.append(title, context);
      if (hit.snippet) {
        const snippet = document.createElement('span');
        snippet.className = 'omnibar-snippet';
        snippet.textContent = hit.snippet;
        row.append(snippet);
      }
      row.onmouseenter = () => select(i);
      results.append(row);
    }
    status.textContent = data.hits.length
      ? `${data.total} matching ${data.total === 1 ? 'entry' : 'entries'}. Showing the most relevant.`
      : 'No matches. Try a shorter word prefix.';
    select(0);
  } catch (error) {
    if (error.name !== 'AbortError' && current === generation)
      status.textContent = error.message;
  }
}
export function openSearch() {
  if (!dialog) {
    dialog = document.createElement('dialog');
    dialog.className = 'search-omnibar';
    dialog.setAttribute('aria-label', 'Quick search');
    const head = document.createElement('div');
    head.className = 'omnibar-heading';
    const label = document.createElement('label');
    label.className = 'sr-only';
    label.htmlFor = 'omnibar-query';
    label.textContent = 'Search everything';
    input = document.createElement('input');
    input.id = 'omnibar-query';
    input.type = 'search';
    input.placeholder = 'Search everything…';
    input.maxLength = 500;
    input.autocomplete = 'off';
    input.role = 'combobox';
    input.setAttribute('aria-autocomplete', 'list');
    input.setAttribute('aria-controls', 'omnibar-results');
    input.setAttribute('aria-expanded', 'true');
    const close = document.createElement('button');
    close.type = 'button';
    close.textContent = 'Esc';
    close.setAttribute('aria-label', 'Close quick search');
    close.onclick = () => dialog.close();
    head.append(label, input, close);
    results = document.createElement('div');
    results.id = 'omnibar-results';
    results.role = 'listbox';
    results.setAttribute('aria-label', 'Quick search results');
    status = document.createElement('p');
    status.className = 'omnibar-status';
    status.role = 'status';
    const footer = document.createElement('footer');
    overview = document.createElement('a');
    overview.textContent = 'View all results & filters →';
    overview.href = '/search?status=all';
    footer.append(overview);
    dialog.append(head, status, results, footer);
    document.body.append(dialog);
    input.oninput = () => {
      clearTimeout(timer);
      controller?.abort();
      generation++;
      timer = setTimeout(search, 220);
    };
    input.onkeydown = (event) => {
      if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
        event.preventDefault();
        select(active + (event.key === 'ArrowDown' ? 1 : -1));
      } else if (event.key === 'Enter') {
        event.preventDefault();
        location.assign(active >= 0 ? results.children[active].href : overview.href);
      }
    };
    dialog.addEventListener('close', () => {
      clearTimeout(timer);
      controller?.abort();
      generation++;
      results.replaceChildren();
      input.value = '';
    });
  }
  if (!dialog.open) dialog.showModal();
  input.focus();
  input.select();
  search();
}
document.addEventListener('click', (event) => {
  if (
    event.button !== 0 ||
    event.metaKey ||
    event.ctrlKey ||
    event.shiftKey ||
    event.altKey
  )
    return;
  if (!event.target.closest('[data-search-open],a[href="/search"]')) return;
  event.preventDefault();
  openSearch();
});
document.addEventListener(
  'keydown',
  (event) => {
    if (
      event.defaultPrevented ||
      event.isComposing ||
      event.altKey ||
      document.querySelector('.confirmation-dialog[open]')
    )
      return;
    const editing = event.target.closest(
      'input,textarea,select,[contenteditable="true"]',
    );
    if (
      ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') ||
      (event.key === '/' && !editing && !event.metaKey && !event.ctrlKey)
    ) {
      event.preventDefault();
      event.stopPropagation();
      openSearch();
    }
  },
  true,
);
