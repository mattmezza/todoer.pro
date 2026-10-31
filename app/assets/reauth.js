let pending;
// Keep the original page (including sensitive, unsaved form values) in memory.
export function reauthenticate() {
  if (pending) return pending;
  pending = new Promise((resolve) => {
    const dialog = document.createElement('dialog');
    dialog.className = 'confirm-dialog';
    const title = document.createElement('h2');
    title.id = 'reauth-title';
    title.textContent = 'Sign in again to continue';
    dialog.setAttribute('aria-labelledby', title.id);
    const message = document.createElement('p');
    message.textContent =
      'This action needs a fresh sign-in. Sign in in a separate window, then your action will continue here. Your unfinished work stays on this page.';
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
      timer = setInterval(() => {
        if (popup.closed) {
          clearInterval(timer);
          start.disabled = false;
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
    dialog.append(title, message, cancel, start);
    document.body.append(dialog);
    dialog.showModal();
    start.focus();
  }).finally(() => {
    pending = null;
  });
  return pending;
}
