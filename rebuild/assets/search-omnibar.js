// Global quick search uses the same live, authorized ranked search contract.
let dialog, input, results, status, overview, controller, timer;
let generation = 0,
  active = -1;
function showStatus(message, icon = 'info') {
  const graphic = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  graphic.setAttribute('class', 'ui-icon omnibar-status-icon');
  graphic.setAttribute('aria-hidden', 'true');
  const use = document.createElementNS('http://www.w3.org/2000/svg', 'use');
  use.setAttribute('href', `/assets/icons.svg#${icon}`);
  graphic.append(use);
  const text = document.createElement('span');
  text.textContent = message;
  status.replaceChildren(graphic, text);
}
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
    showStatus('Search tasks, spaces, lists, comments, tags and files.');
    return;
  }
  showStatus('Searching…', 'search');
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
      const row = document.createElement('div');
      row.className = 'omnibar-result';
      const cell = document.createElement('div');
      cell.role = 'gridcell';
      const link = document.createElement('a');
      link.href = destination(hit);
      link.tabIndex = -1;
      cell.append(link);
      row.id = `omnibar-result-${i}`;
      row.role = 'row';
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
      link.append(title, context);
      if (hit.snippet) {
        const snippet = document.createElement('span');
        snippet.className = 'omnibar-snippet';
        snippet.textContent = hit.snippet;
        link.append(snippet);
      }
      row.append(cell);
      if (hit.reference) {
        const actions = document.createElement('div');
        actions.role = 'gridcell';
        const copy = document.createElement('button');
        copy.type = 'button';
        copy.className = 'omnibar-copy-reference';
        copy.title = `Copy reference ${hit.reference}`;
        copy.setAttribute('aria-label', copy.title);
        const reference = document.createElement('span');
        reference.textContent = hit.reference;
        copy.innerHTML =
          '<svg class="ui-icon" viewBox="0 0 24 24" aria-hidden="true"><rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V4H4v12h4"/></svg>';
        copy.prepend(reference);
        copy.onclick = async () => {
          try {
            await navigator.clipboard.writeText(hit.reference);
            showStatus(`Copied ${hit.reference}`, 'check');
            copy.dataset.copied = 'true';
            setTimeout(() => delete copy.dataset.copied, 1600);
          } catch {
            showStatus(`Could not copy to clipboard. Reference: ${hit.reference}`);
          }
        };
        actions.append(copy);
        row.append(actions);
      }
      row.onmouseenter = () => select(i);
      results.append(row);
    }
    showStatus(data.hits.length
      ? `${data.total} matching ${data.total === 1 ? 'entry' : 'entries'}. Showing the most relevant.`
      : 'No matches. Try a shorter word prefix.', data.hits.length ? 'info' : 'search');
    select(0);
  } catch (error) {
    if (error.name !== 'AbortError' && current === generation)
      showStatus(error.message);
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
    input.setAttribute('aria-haspopup', 'grid');
    input.setAttribute('aria-controls', 'omnibar-results');
    input.setAttribute('aria-expanded', 'true');
    const close = document.createElement('button');
    close.type = 'button';
    close.innerHTML =
      '<svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#close"/></svg>';
    close.setAttribute('aria-label', 'Close quick search');
    close.onclick = () => dialog.close();
    let outsidePress = false;
    const outside = (event) => {
      const r = dialog.getBoundingClientRect();
      return (
        event.target === dialog &&
        (event.clientX < r.left ||
          event.clientX > r.right ||
          event.clientY < r.top ||
          event.clientY > r.bottom)
      );
    };
    dialog.addEventListener('pointerdown', (event) => {
      outsidePress = outside(event);
    });
    dialog.addEventListener('click', (event) => {
      if (outsidePress && outside(event)) dialog.close();
      outsidePress = false;
    });
    head.append(label, input, close);
    results = document.createElement('div');
    results.id = 'omnibar-results';
    results.role = 'grid';
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
        location.assign(
          active >= 0 ? results.children[active].querySelector('a').href : overview.href,
        );
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

document.addEventListener('click', (event) => {
  if (event.target.closest('[data-open-search]')) {
    event.preventDefault();
    openSearch();
  }
});
