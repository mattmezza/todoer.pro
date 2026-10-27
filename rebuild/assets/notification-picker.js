const field = document.querySelector('[data-muted-lists]');
if (field) {
  const source = field.querySelector('[data-mute-choices]');
  const choices = [...source.querySelectorAll('[data-mute-label]')].map((label) => ({
    input: label.querySelector('input'),
    title: label.querySelector('[data-mute-title]').textContent,
    space: label.dataset.space,
  }));
  source.hidden = true;
  const chips = document.createElement('div');
  chips.className = 'muted-list-chips';
  const label = document.createElement('label');
  label.className = 'sr-only';
  label.htmlFor = 'muted-list-search';
  label.textContent = 'Search lists to mute';
  const search = document.createElement('input');
  search.type = 'search';
  search.id = 'muted-list-search';
  search.placeholder = 'Search by list or space…';
  search.autocomplete = 'off';
  search.setAttribute('aria-controls', 'mute-search-results');
  const results = document.createElement('div');
  results.id = 'mute-search-results';
  results.className = 'mute-search-results';
  const hint = document.createElement('p');
  hint.className = 'muted-list-hint';
  hint.textContent = 'Choose lists to silence. Other lists keep their notifications.';
  field.append(chips, label, search, results, hint);
  function selected() {
    chips.replaceChildren();
    for (const choice of choices.filter((item) => item.input.checked)) {
      const pill = document.createElement('span');
      pill.className = 'muted-list-pill';
      const name = document.createElement('span');
      name.textContent = choice.title;
      const space = document.createElement('small');
      space.textContent = choice.space;
      const remove = document.createElement('button');
      remove.type = 'button';
      remove.setAttribute('aria-label', `Unmute ${choice.title} in ${choice.space}`);
      remove.textContent = '×';
      remove.onclick = () => {
        choice.input.checked = false;
        choice.input.dispatchEvent(new Event('change', { bubbles: true }));
        selected();
        showResults();
        search.focus();
      };
      pill.append(name, space, remove);
      chips.append(pill);
    }
    chips.hidden = !chips.children.length;
  }
  function showResults() {
    results.replaceChildren();
    const query = search.value.trim().toLocaleLowerCase();
    if (!query) {
      results.hidden = true;
      return;
    }
    const matches = choices.filter(
      (item) =>
        !item.input.checked &&
        `${item.title} ${item.space}`.toLocaleLowerCase().includes(query),
    );
    for (const choice of matches.slice(0, 12)) {
      const button = document.createElement('button');
      button.type = 'button';
      const title = document.createElement('span');
      title.textContent = choice.title;
      const space = document.createElement('small');
      space.textContent = choice.space;
      button.append(title, space);
      button.onclick = () => {
        choice.input.checked = true;
        choice.input.dispatchEvent(new Event('change', { bubbles: true }));
        search.value = '';
        selected();
        showResults();
        search.focus();
      };
      results.append(button);
    }
    if (!matches.length) {
      const empty = document.createElement('p');
      empty.textContent = 'No unmuted lists match your search.';
      results.append(empty);
    }
    if (matches.length > 12) {
      const more = document.createElement('p');
      more.textContent = 'Showing 12 results. Keep typing to narrow your search.';
      results.append(more);
    }
    results.hidden = false;
  }
  search.addEventListener('input', showResults);
  search.addEventListener('keydown', (event) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      results.querySelector('button')?.click();
    } else if (event.key === 'ArrowDown') {
      event.preventDefault();
      results.querySelector('button')?.focus();
    } else if (event.key === 'Escape') {
      search.value = '';
      showResults();
    }
  });
  selected();
  showResults();
}
