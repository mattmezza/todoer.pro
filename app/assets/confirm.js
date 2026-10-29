// One accessible, styled confirmation surface for every destructive action.
// Queue callers so a second action cannot steal the first dialog's response.
let pending = Promise.resolve();
export function confirmAction(message, options = {}) {
  const next = pending.then(
    () =>
      new Promise((resolve) => {
        const dialog = document.createElement('dialog');
        dialog.className = 'quick-dialog confirmation-dialog';
        dialog.setAttribute('aria-labelledby', 'confirmation-heading');
        dialog.setAttribute('aria-describedby', 'confirmation-message');
        const title = document.createElement('h2');
        title.id = 'confirmation-heading';
        title.textContent = options.title || 'Confirm change';
        const text = document.createElement('p');
        text.id = 'confirmation-message';
        text.textContent = message;
        const form = document.createElement('form');
        form.method = 'dialog';
        form.className = 'confirmation-actions';
        const cancel = document.createElement('button');
        cancel.type = 'submit';
        cancel.value = 'cancel';
        cancel.textContent = 'Cancel';
        cancel.autofocus = true;
        const accept = document.createElement('button');
        accept.type = 'submit';
        accept.value = 'confirm';
        const destructive = /delete|discard|archive|trash|sign out/i.test(message);
        accept.className = destructive ? 'primary destructive' : 'primary';
        accept.textContent = /delete/i.test(message)
          ? 'Delete'
          : /archive/i.test(message)
            ? 'Archive'
            : /trash/i.test(message)
              ? 'Move to trash'
              : /discard/i.test(message)
                ? 'Discard'
                : 'Confirm';
        if (options.acceptLabel) accept.textContent = options.acceptLabel;
        form.append(cancel, accept);
        dialog.append(title, text, form);
        document.body.append(dialog);
        dialog.addEventListener(
          'close',
          () => {
            const accepted = dialog.returnValue === 'confirm';
            dialog.remove();
            resolve(accepted);
          },
          { once: true },
        );
        dialog.showModal();
      }),
  );
  pending = next.catch(() => false);
  return next;
}
