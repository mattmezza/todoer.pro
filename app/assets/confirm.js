import Alpine from './alpine-csp.min.js';

// Queue callers so a second action cannot steal the first dialog's response.
let pending = Promise.resolve();
export function confirmAction(message, options = {}) {
  const next = pending.then(() => {
    const dialog = document.querySelector('[data-confirmation-dialog]');
    if (!dialog) return false;
    return Alpine.$data(dialog).present(message, options);
  });
  pending = next.catch(() => false);
  return next;
}
