// Dock preferences do not require loading the settings panel and its features.
const docks = new WeakMap();
export function dockState(root, kind = 'task', id = root.dataset.taskId) {
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
export function renderDock(state) {
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
      openDock(state);
    }
  };
  renderDock(state);
  if (state.docked && state.expanded) openDock(state);
}
async function openDock(state) {
  const module = await import('./list-settings.js');
  if (state.kind === 'task') module.openTaskSettings(state.root);
  else if (state.kind === 'workspace') module.openWorkspaceSettings(state.id);
  else module.openListSettings(state.id);
}
