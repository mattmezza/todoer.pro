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
  if (initial) target.dataset.loadingPending = variant;
  target.setAttribute('aria-busy', 'true');
  if (initial) target.inert = true;
  let finished = false;
  const state = {
    finish() {
      if (finished) return;
      finished = true;
      if (owners.get(target) !== state) return;
      delete target.dataset.loadingPending;
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
  close.innerHTML = '<svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#close"/></svg>';
  close.onclick = () => dialog.close();
  const body = document.createElement('div');
  const skeleton = beginLoading(body, { variant: 'form' });
  dialog.append(close, body);
  document.body.append(dialog);
  dialog.showModal();
  return { dialog, finish() { skeleton.finish(); dialog.remove(); } };
}
