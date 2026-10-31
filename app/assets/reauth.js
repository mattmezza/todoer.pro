let pending;
// Keep the original page (including sensitive, unsaved form values) in memory.
export function reauthenticate() {
  if (pending) return pending;
  pending = new Promise((resolve) => {
    const dialog = document.createElement('dialog');
    dialog.className = 'quick-dialog confirmation-dialog reauth-dialog';
    const title = document.createElement('h2');
    title.id = 'reauth-title';
    title.innerHTML =
      '<span class="modal-heading-icon" aria-hidden="true"><svg class="ui-icon" width="20" height="20"><use href="/assets/icons.svg#shield"/></svg></span>Confirm it’s you';
    dialog.setAttribute('aria-labelledby', title.id);
    const message = document.createElement('p');
    message.id = 'reauth-message';
    message.textContent =
      'Sign in again to continue this action. Your work will stay here.';
    dialog.setAttribute('aria-describedby', message.id);
    const hint = document.createElement('p');
    hint.className = 'reauth-hint';
    hint.textContent = 'A secure sign-in window will open.';
    hint.setAttribute('role', 'status');
    const actions = document.createElement('div');
    actions.className = 'confirmation-actions';
    const start = document.createElement('button');
    start.type = 'button';
    start.className = 'primary';
    start.textContent = 'Sign in';
    const cancel = document.createElement('button');
    cancel.type = 'button';
    cancel.textContent = 'Cancel';
    let popup, timer;
    const finish = (value) => {
      clearInterval(timer);
      popup?.close();
      dialog.close();
      dialog.remove();
      resolve(value);
    };
    cancel.onclick = () => finish(false);
    dialog.addEventListener('cancel', (event) => {
      event.preventDefault();
      finish(false);
    });
    start.onclick = () => {
      popup = window.open(
        '/signin?next=' + encodeURIComponent('/today?reauth=complete'),
        'todoer-reauth',
        'popup,width=520,height=740',
      );
      if (!popup) {
        message.textContent =
          'Allow the sign-in popup for this site, then select Sign in again. Your work is still here.';
        return;
      }
      start.disabled = true;
      start.textContent = 'Waiting for sign-in…';
      hint.textContent = 'Finish signing in in the other window to continue.';
      timer = setInterval(() => {
        if (popup.closed) {
          clearInterval(timer);
          start.disabled = false;
          start.textContent = 'Sign in';
          hint.textContent = 'A secure sign-in window will open.';
          return;
        }
        try {
          if (
            popup.location.origin !== location.origin ||
            popup.location.pathname !== '/today' ||
            popup.location.search !== '?reauth=complete'
          )
            return;
          const account = document.querySelector('meta[name="account-id"]')?.content;
          const signedIn = popup.document.querySelector(
            'meta[name="account-id"]',
          )?.content;
          const csrf = popup.document.querySelector('meta[name="csrf-token"]')?.content;
          if (!signedIn || !csrf) return;
          if (account !== signedIn) {
            clearInterval(timer);
            popup.close();
            start.disabled = false;
            start.textContent = 'Sign in';
            hint.textContent = 'A secure sign-in window will open.';
            message.textContent =
              'Sign in with the same account to continue this action.';
            return;
          }
          document.querySelector('meta[name="csrf-token"]').content = csrf;
          finish(csrf);
        } catch {
          /* The identity provider is on another origin. */
        }
      }, 500);
    };
    actions.append(cancel, start);
    dialog.append(title, message, hint, actions);
    document.body.append(dialog);
    dialog.showModal();
    start.focus();
  }).finally(() => {
    pending = null;
  });
  return pending;
}
