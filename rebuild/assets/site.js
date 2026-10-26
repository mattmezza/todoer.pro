import './search-omnibar.js';
import { confirmAction } from './confirm.js';
import { listPending, clearAccount } from './offline.js';
import { clearPwa } from './pwa.js';
const navigation = document.querySelector('#navigation');
const toggle = document.querySelector('#nav-toggle');
const mobile = matchMedia('(max-width: 640px)');
let returnFocus = null;
const backdrop = document.createElement('button');
backdrop.className = 'navigation-backdrop';
backdrop.hidden = true;
backdrop.setAttribute('aria-label', 'Close navigation');
document.body.append(backdrop);
function drawer(open) {
  if (!navigation) return;
  if (open && !mobile.matches) return;
  navigation.classList.toggle('open', open);
  toggle?.setAttribute('aria-expanded', String(open));
  backdrop.hidden = !open;
  document.body.classList.toggle('navigation-open', open);
  document.querySelector('main')?.toggleAttribute('inert', open);
  if (open) {
    returnFocus = document.activeElement;
    navigation.querySelector('a,button,summary')?.focus();
  } else {
    returnFocus?.focus({ preventScroll: true });
    returnFocus = null;
  }
}
toggle?.addEventListener('click', () => drawer(!navigation?.classList.contains('open')));
backdrop.addEventListener('click', () => drawer(false));
mobile.addEventListener('change', () => {
  if (!mobile.matches) drawer(false);
});
navigation?.addEventListener('click', (event) => {
  if (event.target.closest('a') && mobile.matches) drawer(false);
});
document.addEventListener('click', (event) => {
  for (const menu of document.querySelectorAll(
    '.account-menu[open],.entity-menu[open],.task-actions[open],.workspace-menu[open]',
  ))
    if (!menu.contains(event.target)) menu.open = false;
});
document.addEventListener('keydown', (event) => {
  if (event.defaultPrevented) return;
  if (mobile.matches && navigation?.classList.contains('open')) {
    if (event.key === 'Escape' && !navigation.querySelector('.account-menu[open]')) {
      event.preventDefault();
      drawer(false);
      return;
    }
    if (event.key === 'Tab') {
      const visible = [...navigation.querySelectorAll('a,button,summary')].filter(
        (el) => el.getClientRects().length && !el.disabled,
      );
      const first = visible[0],
        last = visible.at(-1);
      if (
        event.shiftKey &&
        (document.activeElement === first || !navigation.contains(document.activeElement))
      ) {
        event.preventDefault();
        last?.focus();
      } else if (
        !event.shiftKey &&
        (document.activeElement === last || !navigation.contains(document.activeElement))
      ) {
        event.preventDefault();
        first?.focus();
      }
    }
  }
  const form = event.target.closest?.('form');
  if (event.key === 'Enter' && (event.metaKey || event.ctrlKey) && form) {
    event.preventDefault();
    if (!form.querySelector('button[type="submit"]:disabled')) form.requestSubmit();
  } else if (event.key === 'Escape') {
    if (form?.querySelector('[data-capture-cancel],#capture-cancel')) {
      event.preventDefault();
      form.querySelector('[data-capture-cancel],#capture-cancel').click();
    } else if (form?.matches('.inline-entity-form')) {
      event.preventDefault();
      const cancel = form.querySelector(
        '[data-list-edit-cancel],[data-section-add-cancel]',
      );
      if (cancel) cancel.click();
      else form.hidden = true;
    } else if (form?.closest('.workspace-title-editor')) {
      event.preventDefault();
      form.closest('details').open = false;
    } else {
      for (const menu of document.querySelectorAll(
        '.account-menu[open],.entity-menu[open],.task-actions[open],.workspace-menu[open]',
      )) {
        menu.open = false;
        menu.querySelector('summary')?.focus();
      }
    }
  }
});
document
  .querySelector('.account-menu form[action="/logout"]')
  ?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const account = document.querySelector('meta[name="account-id"]')?.content;
    const csrf = new FormData(event.target).get('csrf');
    try {
      const pending = account ? await listPending(account) : [];
      if (
        pending.length &&
        !(await confirmAction(
          `${pending.length} captures have not synced. Sign out and discard their local copies?`,
        ))
      )
        return;
      const response = await fetch('/logout', {
        method: 'POST',
        headers: { 'X-CSRF-Token': csrf },
        credentials: 'same-origin',
      });
      if (!response.ok)
        throw new Error('Reconnect to sign out. Your drafts remain on this device.');
      document.dispatchEvent(new Event('todoer:logout'));
      if (account) {
        await clearAccount(account);
        await clearPwa(account);
      }
      for (const key of Object.keys(sessionStorage))
        if (key.startsWith('todoer:')) sessionStorage.removeItem(key);
      location.assign('/signin');
    } catch (error) {
      let status = document.querySelector('#notice');
      if (!status) {
        status = document.createElement('p');
        status.setAttribute('role', 'alert');
        event.target.append(status);
      }
      status.textContent = error.message;
    }
  });

const listDialog = document.querySelector('#list-create-dialog');
document.addEventListener('click', (event) => {
  if (event.target.closest('[data-list-create-open]') && listDialog) {
    event.preventDefault();
    document.querySelector('.account-menu')?.removeAttribute('open');
    const form = listDialog.querySelector('form');
    if (!form.elements.workspace_id.value) {
      location.assign('/spaces');
      return;
    }
    listDialog.showModal();
    form.elements.title.focus();
  }
  if (event.target.closest('[data-list-create-close]')) listDialog?.close();
});
listDialog?.querySelector('form').addEventListener('submit', async (event) => {
  event.preventDefault();
  const form = event.target,
    button = form.querySelector('[type="submit"]');
  if (button.disabled) return;
  button.disabled = true;
  const data = new FormData(form);
  try {
    const response = await fetch('/api/v1/lists', {
      method: 'POST',
      credentials: 'same-origin',
      headers: {
        'Content-Type': 'application/json',
        'X-CSRF-Token': document.querySelector('meta[name="csrf-token"]')?.content,
      },
      body: JSON.stringify({
        workspace_id: data.get('workspace_id'),
        title: data.get('title'),
        description: data.get('description'),
        color: data.get('color'),
        restricted: false,
      }),
    });
    const result = await response.json();
    if (!response.ok) throw Error(result.error?.message || 'Could not create list');
    location.assign(`/lists/${result.id}`);
  } catch (error) {
    form.querySelector('[role="alert"]').textContent = error.message;
  } finally {
    button.disabled = false;
  }
});

for (const image of document.querySelectorAll('[data-profile-avatar]'))
  image.addEventListener('error', () => {
    image.hidden = true;
  });

function applyMenuIcons() {
  for (const grip of document.querySelectorAll('.task-drag-grip svg:has(use)')) {
    grip.replaceChildren();
    grip.setAttribute('viewBox', '0 0 24 24');
    for (const x of [9, 15]) for (const y of [6, 12, 18]) {
      const dot = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
      dot.setAttribute('cx', x);
      dot.setAttribute('cy', y);
      dot.setAttribute('r', '1.5');
      dot.setAttribute('fill', 'currentColor');
      grip.append(dot);
    }
  }
  const names = [
    [/settings/i, 'settings'],
    [/^open$/i, 'open'],
    [/^move up$/i, 'arrow-up'],
    [/^move down$/i, 'arrow-down'],
    [/rename|edit/i, 'edit'],
    [/copy|permalink/i, 'link'],
    [/move|drag/i, 'move'],
    [/shar/i, 'user'],
    [/archiv|delet/i, 'trash'],
    [/complete/i, 'check'],
  ];
  for (const button of document.querySelectorAll('.menu-items button,.menu-items a')) {
    if (button.matches('[data-list-archive],[data-list-settings-open],[data-list-sharing-open],[data-workspace-settings-open],[data-list-edit-open],[data-section-edit-open]') && !button.textContent.trim().endsWith('…'))
      button.append(document.createTextNode('…'));
    if (button.querySelector('svg')) continue;
    const icon = names.find(([pattern]) => pattern.test(button.textContent))?.[1];
    if (!icon) continue;
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg'),
      use = document.createElementNS('http://www.w3.org/2000/svg', 'use');
    svg.classList.add('ui-icon');
    svg.setAttribute('aria-hidden', 'true');
    use.setAttribute('href', `/assets/icons.svg#${icon}`);
    svg.append(use);
    button.prepend(svg);
  }
}
function applyEntityColors() {
  applyMenuIcons();
  const hex = /^#[0-9a-f]{6}$/i;
  for (const el of document.querySelectorAll('[data-entity-color]'))
    if (hex.test(el.dataset.entityColor))
      el.style.backgroundColor = el.dataset.entityColor;
  if (hex.test(document.body.dataset.listColor || ''))
    document.body.style.setProperty('--list-color', document.body.dataset.listColor);
  for (const el of document.querySelectorAll('[data-progress]'))
    el.style.setProperty(
      '--progress',
      `${Math.max(0, Math.min(100, Number(el.dataset.progress) || 0))}%`,
    );
}
applyEntityColors();
document.body.addEventListener('htmx:afterSwap', applyEntityColors);

document.addEventListener('click', async (event) => {
  const button = event.target.closest('[data-workspace-settings-open]');
  if (!button) return;
  event.preventDefault();
  button.closest('details')?.removeAttribute('open');
  const { openWorkspaceSettings } = await import('./list-settings.js');
  await openWorkspaceSettings(button.dataset.workspaceSettingsOpen);
});

// Resize initial fields as well as dynamically mounted panels and drafts.
const textareaSizes = new WeakMap();
function growTextarea(field) {
  if (!(field instanceof HTMLTextAreaElement) || !field.getClientRects().length) return;
  const width = field.getBoundingClientRect().width;
  const previous = textareaSizes.get(field);
  if (previous?.value === field.value && previous.width === width) return;
  field.style.height = 'auto';
  const borders = field.offsetHeight - field.clientHeight;
  field.style.height = `${field.scrollHeight + borders}px`;
  textareaSizes.set(field, { value: field.value, width });
}
function growTextareas() {
  document.querySelectorAll('textarea').forEach(growTextarea);
}
document.addEventListener('input', (event) => growTextarea(event.target));
window.addEventListener('resize', growTextareas);
const textareaObserver = new MutationObserver(() => requestAnimationFrame(growTextareas));
textareaObserver.observe(document.body, {
  subtree: true,
  childList: true,
  attributes: true,
  attributeFilter: ['hidden', 'open', 'class'],
});
growTextareas();

// Keep contextual creation explicit, including pages rendered before restart.
for (const button of document.querySelectorAll('.sidebar-add[data-list-create-open]')) {
  button.setAttribute('aria-label', 'Add list');
  if (!button.querySelector('span')) {
    const label = document.createElement('span');
    label.textContent = 'Add list';
    button.append(label);
  }
}
document.querySelector('.primary-navigation a[href="/search"]')?.remove();

for (const link of document.querySelectorAll('.pagination a')) {
  if (link.querySelector('svg')) continue;
  const previous = link.textContent.trim() === 'Previous';
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.classList.add('ui-icon', 'pagination-chevron');
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.setAttribute('fill', 'none');
  svg.setAttribute('stroke', 'currentColor');
  svg.setAttribute('stroke-width', '1.5');
  svg.setAttribute('aria-hidden', 'true');
  const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
  path.setAttribute('d', previous ? 'm15 18-6-6 6-6' : 'm9 6 6 6-6 6');
  path.setAttribute('stroke-linecap', 'round');
  path.setAttribute('stroke-linejoin', 'round');
  svg.append(path);
  if (previous) link.prepend(svg);
  else link.append(svg);
}

for (const link of document.querySelectorAll('.planning-tabs a'))
  if (new URL(link.href).pathname === location.pathname)
    link.setAttribute('aria-current', 'page');

import './quick-capture.js';
