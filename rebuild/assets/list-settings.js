import { mountActivity } from './activity-panel.js';
export function openListSettings(listId, selected = 'sharing') {
  return openSettings(listId, selected, 'list');
}
export function openWorkspaceSettings(id, selected = 'details') {
  return openSettings(id, selected, 'workspace');
}
export function openTaskSettings(root, selected = 'planning') {
  return openSettings(root.dataset.taskId, selected, 'task', root);
}
async function openSettings(listId, selected, kind, taskRoot) {
  const workspace = kind === 'workspace';
  const task = kind === 'task';
  if (!workspace && !task && selected !== 'sharing') selected = 'sharing';
  const stash = taskRoot?.querySelector('[data-task-advanced]');
  const heading = task ? 'Task details' : workspace ? 'Space settings' : 'List settings';
  if (document.querySelector('.list-settings-panel')) return;
  const dialog = document.createElement('dialog');
  dialog.className = 'list-settings-panel';
  dialog.setAttribute('aria-label', heading);
  const header = document.createElement('header'),
    back = document.createElement('button'),
    title = document.createElement('h2');
  back.type = 'button';
  back.textContent = '← Back';
  title.textContent = heading;
  header.append(back, title);
  const tabs = document.createElement('div');
  tabs.className = 'settings-tabs';
  tabs.setAttribute('role', 'tablist');
  const content = document.createElement('div');
  content.className = 'settings-content';
  const status = document.createElement('p');
  status.setAttribute('role', 'status');
  let doc;
  let stopActivity = () => {};
  function choose(tab) {
    stopActivity();
    selected = tab;
    for (const button of tabs.children) {
      button.setAttribute('aria-selected', String(button.dataset.tab === tab));
      button.tabIndex = button.dataset.tab === tab ? 0 : -1;
    }
    if (task)
      for (const node of [...content.children])
        if (node.dataset.panelSection) stash.append(node);
    content.replaceChildren();
    if (tab === 'activity') {
      stopActivity = mountActivity(content, workspace ? 'workspace' : 'task', listId);
      return;
    }
    const metadataTab = task && ['tags', 'structure', 'references'].includes(tab);
    const section = doc?.querySelector(`[data-panel-section="${metadataTab ? 'references' : tab}"]`);
    if (section) {
      const mounted = task ? section : section.cloneNode(true);
      if (!task && tab === 'sharing') {
        for (const link of mounted.querySelectorAll('a')) {
          if (/^Move list to another workspace$|^Open parent workspace$/.test(link.textContent.trim()))
            link.remove();
        }
        for (const form of mounted.querySelectorAll('form[action$="/grants"]')) {
          const trigger = document.createElement('details');
          trigger.className = 'sharing-action';
          const summary = document.createElement('summary');
          summary.textContent = 'Add or change a direct share…';
          const previous = form.previousElementSibling;
          if (previous?.tagName === 'H3') previous.remove();
          form.before(trigger);
          trigger.append(summary, form);
        }
      }
      if (metadataTab) mounted.dataset.metadataTab = tab;
      if (task && (tab === 'planning' || metadataTab)) {
        for (const disclosure of mounted.querySelectorAll(
          '[data-scheduling] > details,[data-task-metadata] > details,[data-reference-details]',
        )) {
          disclosure.open = true;
          disclosure.dataset.panelExpanded = '';
        }
      }
      content.append(mounted);
    }
    else content.textContent = 'Only space owners can change these settings.';
  }
  for (const [tab, label] of task
    ? [
        ['planning', 'Planning'],
        ['tags', 'Tags'],
        ['structure', 'Structure'],
        ['references', 'References'],
        ['activity', 'Activity'],
      ]
    : workspace
      ? [['details', 'Details'], ['sharing', 'Sharing'], ['activity', 'Activity']]
      : [['sharing', 'Sharing']]) {
    const button = document.createElement('button');
    button.type = 'button';
    button.role = 'tab';
    button.dataset.tab = tab;
    button.textContent = label;
    button.onclick = () => choose(tab);
    tabs.append(button);
  }
  tabs.addEventListener('keydown', (event) => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    const options = [...tabs.children];
    const current = options.indexOf(document.activeElement);
    const index =
      event.key === 'Home'
        ? 0
        : event.key === 'End'
          ? options.length - 1
          : (current + (event.key === 'ArrowRight' ? 1 : -1) + options.length) %
            options.length;
    choose(options[index].dataset.tab);
    options[index].focus();
  });
  async function load() {
    if (task) {
      doc = taskRoot;
      choose(selected);
      return;
    }
    const response = await fetch(
      workspace ? `/workspaces/${listId}/settings` : `/lists/${listId}/sharing`,
      { cache: 'no-store' },
    );
    if (!response.ok) throw Error('Could not load current settings');
    doc = new DOMParser().parseFromString(await response.text(), 'text/html');
    choose(selected);
  }
  dialog.append(header, tabs, status, content);
  (taskRoot || document.body).append(dialog);
  dialog.showModal();
  const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;
  if (!reducedMotion)
    dialog.animate(
      [
        { transform: 'translateX(100%)', opacity: 0.6 },
        { transform: 'translateX(0)', opacity: 1 },
      ],
      { duration: 180, easing: 'cubic-bezier(.2,.8,.2,1)' },
    );
  let closing = false;
  async function closePanel() {
    if (closing) return;
    closing = true;
    if (!reducedMotion)
      await dialog
        .animate(
          [
            { transform: 'translateX(0)', opacity: 1 },
            { transform: 'translateX(100%)', opacity: 0.6 },
          ],
          { duration: 140, easing: 'ease-in', fill: 'forwards' },
        )
        .finished.catch(() => {});
    dialog.close();
  }
  history.pushState({ todoerSettings: true }, '', `#settings-${selected}`);
  const pop = () => {
    if (!location.hash.startsWith('#settings-')) closePanel();
  };
  window.addEventListener('popstate', pop);
  back.onclick = () => history.back();
  dialog.addEventListener('cancel', (event) => {
    event.preventDefault();
    history.back();
  });
  dialog.addEventListener('close', () => {
    stopActivity();
    window.removeEventListener('popstate', pop);
    if (task)
      for (const node of [...content.children])
        if (node.dataset.panelSection) stash.append(node);
    dialog.remove();
  });
  dialog.addEventListener('submit', async (event) => {
    if (task) return;
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
