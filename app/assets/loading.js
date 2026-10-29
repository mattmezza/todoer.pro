function part(className, ...children) {
  const element = document.createElement('span');
  element.className = className;
  element.append(...children);
  return element;
}
function lines() {
  return part(
    'skeleton-copy',
    part('skeleton-line'),
    part('skeleton-line skeleton-line-short'),
  );
}
function row() {
  return part('skeleton-row', part('skeleton-icon'), lines());
}
function placeholder(variant, target) {
  const root = document.createElement('div');
  root.className = `loading-skeleton skeleton-${variant}`;
  root.setAttribute('aria-hidden', 'true');
  if (variant === 'text') {
    root.append(part('skeleton-line'));
  } else if (variant === 'discussion') {
    root.append(part('skeleton-line skeleton-line-label'));
    root.append(
      part('skeleton-files', part('skeleton-file', row()), part('skeleton-file', row())),
    );
    root.append(part('skeleton-action'), row());
  } else if (variant === 'calendar') {
    for (let day = 0; day < 7; day++) {
      const column = part('skeleton-day', part('skeleton-line skeleton-line-short'));
      for (let slot = 0; slot < 3; slot++)
        column.append(part('skeleton-calendar-slot', lines()));
      root.append(column);
    }
  } else if (variant === 'cards' || variant === 'choices') {
    for (let i = 0; i < (variant === 'choices' ? 2 : 4); i++)
      root.append(part('skeleton-card', row(), lines(), part('skeleton-action')));
  } else if (variant === 'planning') {
    root.append(row(), row());
    const priorities = part('skeleton-priorities');
    for (let i = 0; i < 4; i++) priorities.append(part('skeleton-action'));
    root.append(part('skeleton-line skeleton-line-label'), priorities, row(), row());
  } else if (variant === 'form') {
    const controls = [
      ...target.querySelectorAll('input:not([type=hidden]),select,textarea'),
    ].filter(
      (control) => control.getClientRects().length && !control.closest('[hidden]'),
    );
    if (controls.length) {
      for (const control of controls) {
        if (control.type === 'checkbox' || control.type === 'radio') {
          const check = row();
          check.classList.add('skeleton-check');
          root.append(check);
        } else
          root.append(
            part(
              'skeleton-field',
              part('skeleton-line skeleton-line-label'),
              part('skeleton-input', part('skeleton-line skeleton-line-short')),
            ),
          );
      }
    } else {
      for (let i = 0; i < 3; i++)
        root.append(
          part(
            'skeleton-field',
            part('skeleton-line skeleton-line-label'),
            part('skeleton-input', part('skeleton-line skeleton-line-short')),
          ),
        );
    }
  } else if (variant === 'capture' || variant === 'task') {
    root.append(part('skeleton-line skeleton-title'), lines());
    if (variant === 'task') root.append(row(), row(), row());
    else
      root.append(
        part(
          'skeleton-capture-footer',
          part('skeleton-line skeleton-line-label'),
          part('skeleton-action'),
        ),
      );
  } else {
    for (let i = 0; i < (variant === 'compact' ? 1 : 3); i++) root.append(row());
  }
  return root;
}
// Initial loads conceal unconfirmed values; refreshes keep usable content intact.
const owners = new WeakMap();
export function beginLoading(target, { variant = 'rows', preserve = false } = {}) {
  if (!target) return { finish() {}, fail() {} };
  owners.get(target)?.finish();
  target.querySelector(':scope > .loading-error')?.remove();
  delete target.dataset.loadFailed;
  const priorBusy = target.getAttribute('aria-busy');
  const priorInert = target.inert;
  const initial = !preserve;
  let skeleton;
  if (initial) {
    target.dataset.loadingPending = variant;
    skeleton = placeholder(variant, target);
    target.append(skeleton);
  }
  target.setAttribute('aria-busy', 'true');
  if (initial) target.inert = true;
  let finished = false;
  const state = {
    finish() {
      if (finished) return;
      finished = true;
      if (owners.get(target) !== state) return;
      delete target.dataset.loadingPending;
      skeleton?.remove();
      target.inert = priorInert;
      if (priorBusy === null) target.removeAttribute('aria-busy');
      else target.setAttribute('aria-busy', priorBusy);
      owners.delete(target);
    },
    fail(message, retry) {
      if (finished || owners.get(target) !== state) return;
      state.finish();
      if (!initial) return;
      target.dataset.loadFailed = variant;
      const error = document.createElement('div');
      error.className = 'loading-error';
      error.setAttribute('role', 'alert');
      const text = document.createElement('p');
      text.textContent = message || 'Could not load this content.';
      error.append(text);
      if (retry) {
        const button = document.createElement('button');
        button.type = 'button';
        button.className = 'secondary';
        button.textContent = 'Try again';
        button.onclick = retry;
        error.append(button);
      }
      target.append(error);
    },
  };
  owners.set(target, state);
  return state;
}

export function loadingDialog(label, className = 'quick-dialog') {
  const dialog = document.createElement('dialog');
  dialog.className = className;
  dialog.setAttribute('aria-label', label);
  const close = document.createElement('button');
  close.type = 'button';
  close.className = 'dialog-close capture-close';
  close.setAttribute('aria-label', 'Close');
  close.innerHTML =
    '<svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#close"/></svg>';
  close.onclick = () => dialog.close();
  const body = document.createElement('div');
  const skeleton = beginLoading(body, { variant: 'capture' });
  dialog.append(close, body);
  document.body.append(dialog);
  dialog.showModal();
  return {
    dialog,
    finish() {
      skeleton.finish();
      dialog.remove();
    },
  };
}
