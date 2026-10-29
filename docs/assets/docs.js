(() => {
  const dialog = document.querySelector('.search-dialog');
  const input = document.querySelector('#docs-search');
  const results = document.querySelector('#search-results');
  const opener = document.querySelector('[data-search-open]');
  let pages;
  let loading;
  let lastFocus;

  const message = (text) => {
    const line = document.createElement('p');
    line.className = 'search-hint';
    line.textContent = text;
    results.replaceChildren(line);
  };
  const load = () => {
    if (!loading) {
      loading = fetch('/search-index.json').then((response) => {
        if (!response.ok) throw new Error('Search unavailable');
        return response.json();
      }).then((index) => { pages = index; }).catch(() => {
        loading = null;
        message('Search could not load. Browse the navigation, or try opening search again.');
      });
    }
    return loading;
  };
  const search = () => {
    if (!pages) return;
    const query = input.value.trim().toLocaleLowerCase();
    if (!query) {
      message('Search tasks, CLI commands, deployment and more.');
      return;
    }
    const words = query.split(/\s+/).filter(Boolean);
    const matches = pages.map((page) => {
      const title = page.title.toLocaleLowerCase();
      const body = page.text.toLocaleLowerCase();
      const haystack = title + ' ' + body;
      const score = words.every((word) => haystack.includes(word))
        ? words.reduce((total, word) => total + (title.includes(word) ? 10 : 1), 0) : 0;
      return { page, score };
    }).filter(({ score }) => score).sort((a, b) => b.score - a.score).slice(0, 12);
    if (!matches.length) {
      message('No guides found. Try “capture”, “login” or “backup”.');
      return;
    }
    const nodes = matches.map(({ page }) => {
      const link = document.createElement('a');
      link.className = 'search-result';
      link.href = page.url;
      const group = document.createElement('span');
      group.textContent = page.group;
      const title = document.createElement('strong');
      title.textContent = page.title;
      const preview = document.createElement('p');
      const firstMatch = page.text.toLocaleLowerCase().indexOf(words[0]);
      const start = Math.max(0, firstMatch - 45);
      preview.textContent = (start ? '…' : '') + page.text.slice(start, start + 155).replace(/\s+/g, ' ') + '…';
      link.append(group, title, preview);
      return link;
    });
    results.replaceChildren(...nodes);
  };
  const open = async () => {
    lastFocus = document.activeElement;
    dialog.showModal();
    input.focus();
    if (!pages) message('Loading documentation…');
    await load();
    search();
  };
  const close = () => dialog.close();
  opener.hidden = false;
  opener.addEventListener('click', open);
  document.querySelector('[data-search-close]').addEventListener('click', close);
  dialog.addEventListener('close', () => { lastFocus?.focus(); });
  dialog.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') { event.preventDefault(); close(); }
  });
  dialog.addEventListener('click', (event) => {
    if (event.target !== dialog) return;
    const rect = dialog.getBoundingClientRect();
    if (event.clientX < rect.left || event.clientX > rect.right || event.clientY < rect.top || event.clientY > rect.bottom) close();
  });
  input.addEventListener('input', search);
  input.addEventListener('keydown', (event) => {
    if (event.key === 'ArrowDown') {
      const first = results.querySelector('a');
      if (first) { event.preventDefault(); first.focus(); }
    } else if (event.key === 'Enter') {
      const first = results.querySelector('a');
      if (first) first.click();
    }
  });
  results.addEventListener('keydown', (event) => {
    if (!['ArrowDown', 'ArrowUp'].includes(event.key)) return;
    const links = Array.from(results.querySelectorAll('a'));
    const position = links.indexOf(document.activeElement);
    if (position < 0) return;
    event.preventDefault();
    const next = position + (event.key === 'ArrowDown' ? 1 : -1);
    if (next < 0) input.focus();
    else links[Math.min(next, links.length - 1)].focus();
  });
  document.addEventListener('keydown', (event) => {
    const editing = event.target.closest('input, textarea, [contenteditable="true"]');
    if (((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') || (event.key === '/' && !editing)) {
      event.preventDefault();
      if (!dialog.open) open();
    }
  });
  if (!navigator.platform.toLowerCase().includes('mac')) {
    opener.querySelector('kbd').textContent = 'Ctrl K';
  }

  if (navigator.clipboard) document.querySelectorAll('.prose pre').forEach((block) => {
    const code = block.querySelector('code');
    if (!code) return;
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'copy-code';
    button.textContent = 'Copy';
    button.setAttribute('aria-label', 'Copy code block');
    button.addEventListener('click', async () => {
      try {
        await navigator.clipboard.writeText(code.textContent);
        button.textContent = 'Copied';
      } catch {
        button.textContent = 'Select to copy';
      }
      setTimeout(() => { button.textContent = 'Copy'; }, 1800);
    });
    block.prepend(button);
  });
})();
