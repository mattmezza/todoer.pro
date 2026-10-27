import { request as fetch } from './request.js';
import { mountPublicSharing } from './public-sharing.js';
import { mountStructure } from './task-structure.js';
import { confirmAction } from './confirm.js';
import { mountActivity } from './activity-panel.js';
// Loaded with the settings module so every entry point shares the panel styles.
if (!document.querySelector('link[data-sharing-settings-style]')) {
  const stylesheet = document.createElement('link');
  stylesheet.rel = 'stylesheet';
  stylesheet.href = '/assets/sharing-settings.css';
  stylesheet.dataset.sharingSettingsStyle = '';
  document.head.append(stylesheet);
}
const docks = new WeakMap();
function dockState(root, kind = 'task', id = root.dataset.taskId) {
  if (!docks.has(root)) docks.set(root, new Map());
  const states = docks.get(root);
  if (!states.has(kind)) {
    const key = `todoer:${root.dataset.accountId || document.querySelector('meta[name=account-id]')?.content || ''}:${kind}-settings-dock${root.dataset.embedded ? '-preview' : ''}`;
    let saved = {};
    try {
      saved = JSON.parse(localStorage.getItem(key) || '{}');
    } catch {}
    states.set(kind, {
      root,
      kind,
      id,
      key,
      docked: saved.docked === true,
      expanded: saved.expanded !== false,
      panel: null,
    });
  }
  return states.get(kind);
}
function renderDock(state) {
  const open = Boolean(state.panel?.open);
  document.body.classList.toggle(
    'task-settings-docked-open',
    Boolean(document.querySelector('.list-settings-panel.is-docked[open]')),
  );
  state.root.dataset.settingsDocked = String(state.docked);
  for (const option of state.root.querySelectorAll(
    '[data-open-advanced],[data-list-settings-open],[data-list-sharing-open],[data-workspace-settings-open]',
  ))
    if (
      (state.kind === 'task' && option.matches('[data-open-advanced]')) ||
      (state.kind === 'list' &&
        option.matches('[data-list-settings-open],[data-list-sharing-open]')) ||
      (state.kind === 'workspace' && option.matches('[data-workspace-settings-open]'))
    )
      option.hidden = state.docked;
  if (state.kind === 'workspace') {
    const menu = state.root.querySelector('.space-overview-heading > .entity-menu');
    if (menu) menu.hidden = state.docked && !menu.querySelector('[data-export-kind]');
  }
  if (state.toggle) {
    state.toggle.hidden = !state.docked;
    state.toggle.setAttribute('aria-expanded', String(open));
    state.toggle.setAttribute(
      'aria-label',
      `${open ? 'Collapse' : 'Show'} ${state.kind === 'workspace' ? 'space' : state.kind} settings`,
    );
    state.toggle.title = `${open ? 'Collapse' : 'Show'} ${state.kind === 'workspace' ? 'space' : state.kind} settings`;
  }
  try {
    localStorage.setItem(
      state.key,
      JSON.stringify({ docked: state.docked, expanded: state.expanded }),
    );
  } catch {}
}
export function setupTaskSettingsDock(root, kind = 'task', id = root.dataset.taskId) {
  const state = dockState(root, kind, id);
  if (state.toggle) return;
  const toggle = document.createElement('button');
  toggle.type = 'button';
  toggle.className = 'task-settings-dock-toggle';
  toggle.innerHTML =
    '<svg class="ui-icon" viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M15 4v16"/></svg>';
  const anchor = root.querySelector(
    '.task-page-heading > .entity-reference,.page-heading > .entity-reference,.workspace-header h1',
  );
  if (!anchor) return;
  anchor.after(toggle);
  state.toggle = toggle;
  toggle.onclick = () => {
    if (state.panel) state.panel.dismissSettings();
    else {
      state.expanded = true;
      openSettings(
        id,
        kind === 'task' ? 'planning' : kind === 'list' ? 'sharing' : 'sharing',
        kind,
        root,
      );
    }
  };
  renderDock(state);
  if (state.docked && state.expanded)
    openSettings(
      id,
      kind === 'task' ? 'planning' : kind === 'list' ? 'sharing' : 'sharing',
      kind,
      root,
    );
}
export function openListSettings(listId, selected = 'sharing') {
  return openSettings(listId, selected, 'list');
}
export function openWorkspaceSettings(id, selected = 'sharing') {
  return openSettings(id, selected, 'workspace');
}
export function openTaskSettings(root, selected = 'planning') {
  return openSettings(root.dataset.taskId, selected, 'task', root);
}
async function openSettings(listId, selected, kind, taskRoot) {
  const workspace = kind === 'workspace';
  const task = kind === 'task';
  if (
    task &&
    (selected === 'references' ||
      (selected === 'structure' && !taskRoot?.dataset.parentId))
  )
    selected = 'planning';
  if (!workspace && !task && selected !== 'sharing') selected = 'sharing';
  const stash = taskRoot?.querySelector('[data-task-advanced]');
  const heading = task ? 'Task details' : workspace ? 'Space settings' : 'List settings';
  taskRoot ||= document.querySelector('main');
  const state = dockState(taskRoot, kind, listId);
  const existing = document.querySelector('.list-settings-panel');
  if (existing) {
    if (existing === state.panel) existing.chooseSettings?.(selected);
    return;
  }
  let docked = Boolean(state?.docked);
  let ignoredCloses = 0;
  let ownsHistory = !docked;
  const dialog = document.createElement('dialog');
  dialog.className = 'list-settings-panel';
  dialog.dataset.settingsKind = kind;
  dialog.setAttribute('aria-label', heading);
  const header = document.createElement('header'),
    back = document.createElement('button'),
    title = document.createElement('h2');
  back.type = 'button';
  back.textContent = '← Back';
  title.textContent = heading;
  header.append(back, title);
  const pin = document.createElement('button');
  {
    pin.hidden = Boolean(taskRoot?.dataset.embedded);
    pin.type = 'button';
    pin.className = 'task-settings-pin';
    pin.innerHTML =
      '<svg class="ui-icon" viewBox="0 0 24 24" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" d="M9 3h6l-1 6 4 4v2H6v-2l4-4-1-6ZM12 15v7"/></svg>';
    header.append(pin);
  }
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
    if (
      task &&
      (tab === 'references' || (tab === 'structure' && !taskRoot.dataset.parentId))
    )
      tab = 'planning';
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
      stopActivity = mountActivity(
        content,
        workspace ? 'workspace' : task ? 'task' : 'list',
        listId,
      );
      return;
    }
    if (task && tab === 'structure') {
      stopActivity = mountStructure(content, listId);
      return;
    }
    const metadataTab = task && tab === 'tags';
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
        if (workspace) decorateMembers(mounted, listId);
        mountPublicSharing(mounted, workspace ? 'space' : 'list', listId);
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
        ...(taskRoot.dataset.parentId ? [['structure', 'Structure']] : []),
        ['activity', 'Activity'],
      ]
    : workspace
      ? [
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
  function presentation() {
    dialog.classList.toggle('is-docked', docked);
    if (docked) dialog.show();
    else dialog.showModal();
    {
      state.panel = dialog;
      state.expanded = true;
      pin.setAttribute(
        'aria-label',
        `${docked ? 'Unpin' : 'Pin'} ${workspace ? 'space' : kind} settings`,
      );
      pin.title = `${docked ? 'Unpin' : 'Pin'} ${workspace ? 'space' : kind} settings`;
      pin.setAttribute('aria-pressed', String(docked));
      back.hidden = docked;
      renderDock(state);
    }
  }
  presentation();
  dialog.chooseSettings = choose;
  pin.onclick = () => {
    opening?.cancel();
    ignoredCloses++;
    dialog.close();
    docked = !docked;
    state.docked = docked;
    if (docked) {
      ownsHistory = false;
      history.replaceState(null, '', location.pathname + location.search);
    } else {
      ownsHistory = true;
      history.pushState({ todoerSettings: true }, '', `#settings-${selected}`);
    }
    presentation();
    pin.focus();
  };
  const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;
  const opening = reducedMotion
    ? null
    : dialog.animate(
        [
          { transform: docked ? 'translateX(100%)' : 'translateX(48px)', opacity: 0 },
          { transform: 'translateX(0)', opacity: 1 },
        ],
        { duration: 260, easing: 'cubic-bezier(.22,.8,.25,1)' },
      );
  let closing = false;
  let dismissalRequested = false;
  function dismiss() {
    if (closing || dismissalRequested) return;
    dismissalRequested = true;
    if (docked) closePanel();
    else history.back();
  }
  dialog.dismissSettings = dismiss;
  async function closePanel() {
    if (closing) return;
    closing = true;
    const current = getComputedStyle(dialog);
    const from = { transform: current.transform, opacity: current.opacity };
    opening?.cancel();
    dialog.dataset.closing = '';
    if (
      docked &&
      ![...document.querySelectorAll('.list-settings-panel.is-docked[open]')].some(
        (panel) => panel !== dialog,
      )
    )
      document.body.classList.remove('task-settings-docked-open');
    if (!reducedMotion)
      await dialog
        .animate(
          [
            from,
            { transform: docked ? 'translateX(100%)' : 'translateX(32px)', opacity: 0 },
          ],
          {
            duration: 260,
            easing: 'cubic-bezier(.4,0,1,1)',
            fill: 'forwards',
          },
        )
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
    if (!docked && backdropPress && outside(event)) dismiss();
    backdropPress = false;
  });
  if (ownsHistory)
    history.pushState({ todoerSettings: true }, '', `#settings-${selected}`);
  const pop = () => {
    if (ownsHistory && !location.hash.startsWith('#settings-')) closePanel();
  };
  window.addEventListener('popstate', pop);
  back.onclick = dismiss;
  dialog.addEventListener('cancel', (event) => {
    event.preventDefault();
    dismiss();
  });
  dialog.addEventListener('close', () => {
    if (ignoredCloses) {
      ignoredCloses--;
      return;
    }
    if (state) {
      state.panel = null;
      state.expanded = false;
      renderDock(state);
    }
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
    const payload = new URLSearchParams(new FormData(form));
    const controls = [...dialog.querySelectorAll('button,input,select,textarea')].filter(
      (el) => !el.disabled,
    );
    for (const control of controls) control.disabled = true;
    status.textContent = 'Saving…';
    try {
      const response = await fetch(form.action, {
        method: 'POST',
        body: payload,
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
    } finally {
      for (const control of controls) control.disabled = false;
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
    link.append('…');
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

async function decorateMembers(section, id) {
  const endpoint = '/api/v1/workspaces/' + encodeURIComponent(id) + '/people';
  try {
    const r = await fetch(endpoint);
    if (r.ok)
      for (const person of await r.json()) {
        const row = [...section.querySelectorAll('[data-member-id]')].find(
          (row) => row.dataset.memberId === person.id,
        );
        if (row) {
          row.querySelector('[data-member-name]').textContent = person.name;
          row.querySelector('[data-member-email]').textContent = person.email;
        }
      }
  } catch {}
  for (const form of section.querySelectorAll('[data-member-role]')) {
    form.querySelector('select').onchange = () => form.requestSubmit();
  }
  for (const form of section.querySelectorAll('[data-member-remove]')) {
    const remove = form.querySelector('button');
    remove.title = 'Remove member…';
    remove.setAttribute('aria-label', remove.getAttribute('aria-label') + '…');
    form.addEventListener('submit', async (event) => {
      if (form.dataset.confirmed) return;
      event.preventDefault();
      event.stopPropagation();
      const name = form
        .closest('[data-member-id]')
        .querySelector('[data-member-name]').textContent;
      if (
        await confirmAction(
          'Remove ' + name + ' from this space? Separate direct list shares remain.',
        )
      ) {
        form.dataset.confirmed = 'true';
        form.requestSubmit();
      }
    });
  }
  const form = section.querySelector('[data-member-add]');
  if (!form) return;
  const input = form.querySelector('input[type=search]');
  const results = form.querySelector('.member-matches');
  const button = form.querySelector('button[type=submit]');
  let timer,
    generation = 0;
  input.oninput = () => {
    clearTimeout(timer);
    const version = ++generation;
    form.elements.user_id.value = '';
    button.disabled = true;
    results.replaceChildren();
    const query = input.value.trim();
    if (query.length < 3) return;
    timer = setTimeout(async () => {
      try {
        const response = await fetch(endpoint + '?q=' + encodeURIComponent(query));
        if (!response.ok) throw Error('Could not search members.');
        const people = await response.json();
        if (version !== generation || !section.isConnected) return;
        for (const person of people) {
          const pick = document.createElement('button');
          pick.type = 'button';
          pick.textContent = person.name + ' · ' + person.email;
          pick.onclick = () => {
            input.value = person.email;
            form.elements.user_id.value = person.id;
            button.disabled = false;
            results.replaceChildren();
          };
          results.append(pick);
        }
        if (!people.length)
          results.textContent =
            'Enter the full email of an existing account, or invite them below.';
      } catch (error) {
        if (version === generation) results.textContent = error.message;
      }
    }, 180);
  };
  input.onkeydown = (event) => {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      results.querySelector('button')?.focus();
    }
    if (event.key === 'Enter' && !form.elements.user_id.value) {
      event.preventDefault();
      results.querySelector('button')?.click();
    }
  };
  form.addEventListener('submit', (event) => {
    if (!form.elements.user_id.value) {
      event.preventDefault();
      event.stopPropagation();
      input.focus();
    }
  });
}
