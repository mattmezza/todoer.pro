const field = document.querySelector('[data-muted-lists]');
if (field) {
  const source = field.querySelector('[data-mute-choices]');
  const choices = [...source.querySelectorAll('[data-mute-label]')].map((label) => ({
    input: label.querySelector('input'),
    title: label.querySelector('[data-mute-title]').textContent,
    space: label.dataset.space,
  }));
  source.hidden = true;
  const count = document.querySelector('[data-muted-count]');
  const searchBox = document.createElement('div');
  searchBox.className = 'muted-list-search';
  searchBox.innerHTML =
    '<svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#search"/></svg><label class="sr-only" for="muted-list-search">Find a list to mute</label>';
  const search = document.createElement('input');
  search.type = 'search';
  search.id = 'muted-list-search';
  search.placeholder = 'Find a list to mute…';
  search.autocomplete = 'off';
  search.setAttribute('aria-controls', 'mute-search-results');
  searchBox.append(search);
  const results = document.createElement('div');
  results.id = 'mute-search-results';
  results.className = 'mute-search-results';
  const selectedRows = document.createElement('div');
  selectedRows.className = 'muted-list-selection';
  const empty = document.createElement('p');
  empty.className = 'muted-list-empty';
  empty.innerHTML =
    '<svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#bell"/></svg><span></span>';
  empty.querySelector('span').textContent = choices.length
    ? 'All your lists can notify you.'
    : 'No lists to mute yet.';
  const announcement = document.createElement('span');
  announcement.className = 'sr-only';
  announcement.setAttribute('role', 'status');
  field.append(searchBox, results, selectedRows, empty, announcement);
  searchBox.hidden = !choices.length;
  function name(choice) {
    const info = document.createElement('span');
    info.className = 'muted-list-name';
    const title = document.createElement('strong');
    title.textContent = choice.title;
    const space = document.createElement('small');
    space.textContent = choice.space;
    info.append(title, space);
    return info;
  }
  function change(choice, muted) {
    choice.input.checked = muted;
    choice.input.dispatchEvent(new Event('change', { bubbles: true }));
    search.value = '';
    selected();
    results.hidden = true;
    search.focus();
    announcement.textContent = `${choice.title} ${muted ? 'muted' : 'unmuted'}. Save preferences to apply.`;
  }
  function selected() {
    selectedRows.replaceChildren();
    const muted = choices.filter((choice) => choice.input.checked);
    for (const choice of muted) {
      const row = document.createElement('div');
      row.className = 'muted-list-row';
      row.innerHTML =
        '<svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#bell"/></svg>';
      const remove = document.createElement('button');
      remove.type = 'button';
      remove.textContent = 'Unmute';
      remove.setAttribute('aria-label', `Unmute ${choice.title} in ${choice.space}`);
      remove.addEventListener('click', () => change(choice, false));
      row.append(name(choice), remove);
      selectedRows.append(row);
    }
    selectedRows.hidden = !muted.length;
    empty.hidden = muted.length > 0;
    count.hidden = !muted.length;
    count.textContent = `${muted.length} muted`;
  }
  function showResults() {
    results.replaceChildren();
    const query = search.value.trim().toLocaleLowerCase();
    const matches = choices.filter(
      (choice) =>
        !choice.input.checked &&
        `${choice.title} ${choice.space}`.toLocaleLowerCase().includes(query),
    );
    for (const choice of matches.slice(0, 8)) {
      const button = document.createElement('button');
      button.type = 'button';
      button.setAttribute('aria-label', `Mute ${choice.title} in ${choice.space}`);
      const action = document.createElement('span');
      action.className = 'mute-list-action';
      action.textContent = 'Mute';
      button.append(name(choice), action);
      button.addEventListener('click', () => change(choice, true));
      results.append(button);
    }
    if (!matches.length || matches.length > 8) {
      const hint = document.createElement('p');
      hint.textContent =
        matches.length > 8
          ? 'Type to narrow down the results.'
          : query
            ? 'No matching lists.'
            : 'All available lists are muted.';
      results.append(hint);
    }
    results.hidden = false;
  }
  search.addEventListener('focus', showResults);
  search.addEventListener('input', showResults);
  search.addEventListener('keydown', (event) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      if (!results.hidden) results.querySelector('button')?.click();
    } else if (event.key === 'ArrowDown') {
      event.preventDefault();
      showResults();
      results.querySelector('button')?.focus();
    } else if (event.key === 'Escape') {
      event.preventDefault();
      search.value = '';
      results.hidden = true;
    }
  });
  field.addEventListener('focusout', () => {
    setTimeout(() => {
      if (!field.contains(document.activeElement)) results.hidden = true;
    }, 0);
  });
  selected();
  results.hidden = true;
}
