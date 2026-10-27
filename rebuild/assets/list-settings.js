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
    const section = doc?.querySelector(
      `[data-panel-section="${metadataTab ? 'references' : tab}"]`,
    );
    if (section) {
      const mounted = task ? section : section.cloneNode(true);
      if (!task && tab === 'sharing') {
        for (const link of mounted.querySelectorAll('a')) {
          if (
            /^Move list to another workspace$|^Open parent workspace$/.test(
              link.textContent.trim(),
            )
          )
            link.remove();
        }
        decorateSharing(mounted, load);
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
    } else content.textContent = 'Only space owners can change these settings.';
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
      ? [
          ['details', 'Details'],
          ['sharing', 'Sharing'],
          ['activity', 'Activity'],
        ]
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
  const opening = reducedMotion
    ? null
    : dialog.animate(
        [
          { transform: 'translateX(48px)', opacity: 0 },
          { transform: 'translateX(0)', opacity: 1 },
        ],
        { duration: 150, easing: 'cubic-bezier(.16,1,.3,1)' },
      );
  let closing = false;
  let dismissalRequested = false;
  function dismiss() {
    if (closing || dismissalRequested) return;
    dismissalRequested = true;
    history.back();
  }
  async function closePanel() {
    if (closing) return;
    closing = true;
    const current = getComputedStyle(dialog);
    const from = { transform: current.transform, opacity: current.opacity };
    opening?.cancel();
    dialog.dataset.closing = '';
    if (!reducedMotion)
      await dialog
        .animate([from, { transform: 'translateX(32px)', opacity: 0 }], {
          duration: 100,
          easing: 'cubic-bezier(.4,0,1,1)',
          fill: 'forwards',
        })
        .finished.catch(() => {});
    dialog.close();
  }
  const outside = (event) => {
    const bounds = dialog.getBoundingClientRect();
    return (
      event.target === dialog &&
      (event.clientX < bounds.left ||
        event.clientX > bounds.right ||
        event.clientY < bounds.top ||
        event.clientY > bounds.bottom)
    );
  };
  let backdropPress = false;
  dialog.addEventListener('pointerdown', (event) => {
    backdropPress = outside(event);
  });
  dialog.addEventListener('pointercancel', () => {
    backdropPress = false;
  });
  dialog.addEventListener('click', (event) => {
    if (backdropPress && outside(event)) dismiss();
    backdropPress = false;
  });
  history.pushState({ todoerSettings: true }, '', `#settings-${selected}`);
  const pop = () => {
    if (!location.hash.startsWith('#settings-')) closePanel();
  };
  window.addEventListener('popstate', pop);
  back.onclick = dismiss;
  dialog.addEventListener('cancel', (event) => {
    event.preventDefault();
    dismiss();
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

function icon(name) {
  const holder = document.createElement('span');
  holder.innerHTML = `<svg class="ui-icon" width="18" height="18" aria-hidden="true"><use href="/assets/icons.svg#${name}"/></svg>`;
  return holder.firstElementChild;
}
function decorateSharing(section, reload) {
  section.classList.add('sharing-panel');
  for (const label of section.querySelectorAll('label:has(input[type="checkbox"])')) {
    const checkbox = label.querySelector('input');
    const text = document.createElement('span');
    for (const node of [...label.childNodes]) if (node !== checkbox) text.append(node);
    label.append(text);
    label.classList.add('sharing-consent');
  }
  for (const heading of section.querySelectorAll('h2,h3')) heading.prepend(icon('user'));
  const grants = section.querySelector('form[action$="/grants"]');
  if (grants) {
    grants.previousElementSibling?.remove();
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'sharing-add';
    button.append(icon('plus'), 'Add or change a direct share…');
    grants.before(button);
    grants.hidden = true;
    button.onclick = () => {
      grants.hidden = false;
      openSharingModal('Direct share', grants, reload, () => {
        grants.hidden = true;
        button.after(grants);
      });
    };
    const note = grants.nextElementSibling;
    if (note?.textContent.includes('Email invitations are not implemented'))
      note.remove();
  }
  for (const link of section.querySelectorAll('a[href^="/invitations/new"]')) {
    link.prepend(icon('plus-circle'));
    link.addEventListener('click', async (event) => {
      event.preventDefault();
      try {
        const response = await fetch(link.href, { cache: 'no-store' });
        if (!response.ok) throw Error('Could not load invitation form.');
        const doc = new DOMParser().parseFromString(await response.text(), 'text/html');
        const form = doc.querySelector('form');
        if (!form) throw Error('Invitation form unavailable.');
        openSharingModal('Invite by email', form, reload);
      } catch (error) {
        link.after(
          Object.assign(document.createElement('p'), {
            textContent: error.message,
            role: 'alert',
          }),
        );
      }
    });
  }
}
function openSharingModal(title, form, reload, restore = () => {}) {
  const modal = document.createElement('dialog');
  modal.className = 'sharing-modal';
  modal.setAttribute('aria-label', title);
  const heading = document.createElement('h2');
  heading.textContent = title;
  const cancel = document.createElement('button');
  cancel.type = 'button';
  cancel.textContent = 'Cancel';
  cancel.onclick = () => modal.close();
  const actions = document.createElement('div');
  actions.className = 'sharing-modal-actions';
  const submit = form.querySelector('button:not([type]),button[type="submit"]');
  actions.append(cancel, submit);
  form.append(actions);
  const status = document.createElement('p');
  status.role = 'alert';
  modal.append(heading, form, status);
  // Keep the modal inside the active panel so it remains interactive above it.
  document.querySelector('.list-settings-panel').append(modal);
  modal.addEventListener('close', () => {
    form.append(submit);
    actions.remove();
    restore();
    modal.remove();
  });
  modal.addEventListener('submit', async (event) => {
    event.preventDefault();
    event.stopPropagation();
    submit.disabled = true;
    try {
      const response = await fetch(form.action, {
        method: 'POST',
        body: new URLSearchParams(new FormData(form)),
      });
      if (!response.ok) throw Error('Could not save. Check the details and try again.');
      if (form.action.includes('/invitations/new')) {
        const doc = new DOMParser().parseFromString(await response.text(), 'text/html');
        const result = doc.querySelector('main');
        result?.querySelector('a')?.remove();
        if (result) {
          form.replaceWith(result);
          cancel.textContent = 'Done';
          modal.append(cancel);
        }
      } else {
        modal.close();
        await reload();
      }
    } catch (error) {
      status.textContent = error.message;
    } finally {
      submit.disabled = false;
    }
  });
  modal.showModal();
  form.querySelector('input:not([type="hidden"])')?.focus();
}
