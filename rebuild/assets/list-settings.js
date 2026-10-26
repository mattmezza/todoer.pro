export async function openListSettings(listId, selected = 'details') {
  if (document.querySelector('.list-settings-panel')) return;
  const dialog = document.createElement('dialog');
  dialog.className = 'list-settings-panel';
  dialog.setAttribute('aria-label', 'List settings');
  const header = document.createElement('header'),
    back = document.createElement('button'),
    title = document.createElement('h2');
  back.type = 'button';
  back.textContent = '← Back';
  title.textContent = 'List settings';
  header.append(back, title);
  const tabs = document.createElement('div');
  tabs.className = 'settings-tabs';
  tabs.setAttribute('role', 'tablist');
  const content = document.createElement('div');
  content.className = 'settings-content';
  const status = document.createElement('p');
  status.setAttribute('role', 'status');
  let doc;
  function choose(tab) {
    selected = tab;
    for (const button of tabs.children)
      button.setAttribute('aria-selected', String(button.dataset.tab === tab));
    content.replaceChildren();
    const section = doc?.querySelector(`[data-panel-section="${tab}"]`);
    if (section) content.append(section.cloneNode(true));
  }
  for (const [tab, label] of [
    ['details', 'Details'],
    ['sharing', 'Sharing'],
    ['sections', 'Sections'],
  ]) {
    const button = document.createElement('button');
    button.type = 'button';
    button.role = 'tab';
    button.dataset.tab = tab;
    button.textContent = label;
    button.onclick = () => choose(tab);
    tabs.append(button);
  }
  async function load() {
    const response = await fetch(`/lists/${listId}/sharing`, { cache: 'no-store' });
    if (!response.ok) throw Error('Could not load current settings');
    doc = new DOMParser().parseFromString(await response.text(), 'text/html');
    choose(selected);
  }
  dialog.append(header, tabs, status, content);
  document.body.append(dialog);
  dialog.showModal();
  history.pushState({ todoerSettings: true }, '', `#settings-${selected}`);
  const pop = () => {
    if (!location.hash.startsWith('#settings-')) dialog.close();
  };
  window.addEventListener('popstate', pop);
  back.onclick = () => history.back();
  dialog.addEventListener('cancel', (event) => {
    event.preventDefault();
    history.back();
  });
  dialog.addEventListener('close', () => {
    window.removeEventListener('popstate', pop);
    dialog.remove();
  });
  dialog.addEventListener('submit', async (event) => {
    const form = event.target;
    if (!(form instanceof HTMLFormElement)) return;
    event.preventDefault();
    const submit = form.querySelector('button[type=submit],button:not([type])');
    if (submit) submit.disabled = true;
    try {
      const response = await fetch(form.action, {
        method: 'POST',
        body: new URLSearchParams(new FormData(form)),
      });
      if (!response.ok) {
        const error = await response.json().catch(() => null);
        throw Error(
          error?.error?.message || 'Could not save. Refresh settings and try again.',
        );
      }
      const color = form.querySelector('[name=color]')?.value;
      await load();
      if (color && /^#[0-9a-f]{6}$/i.test(color)) {
        document.body.dataset.listColor = color;
        document.body.style.setProperty('--list-color', color);
        for (const badge of document.querySelectorAll(
          `.entity-color,.list-navigation[href="/lists/${listId}"] .list-dot`,
        )) {
          badge.dataset.entityColor = color;
          badge.style.backgroundColor = color;
        }
      }
      const edit = document.querySelector('[data-list-edit]');
      if (edit?.hidden) {
        const revision = doc.querySelector('input[name=revision]')?.value;
        if (revision) edit.dataset.revision = revision;
      }
      status.textContent = 'Saved.';
    } catch (error) {
      status.textContent = error.message;
      if (submit) submit.disabled = false;
    }
  });
  try {
    await load();
  } catch (error) {
    status.textContent = error.message;
  }
}
