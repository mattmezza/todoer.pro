export async function setupPwa(scope, onLogout) {
  if (!('serviceWorker' in navigator)) return null;
  try {
    const registration = await navigator.serviceWorker.register('/sw.js', { scope: '/' });
    const notify = () =>
      navigator.serviceWorker.controller?.postMessage({ type: 'SET_SCOPE', scope });
    navigator.serviceWorker.addEventListener('controllerchange', notify);
    navigator.serviceWorker.addEventListener('message', (e) => {
      if (e.data?.type === 'ACCOUNT_CLEARED') onLogout?.();
    });
    await navigator.serviceWorker.ready;
    notify();
    const showUpdate = () => {
      if (!registration.waiting) return;
      let button = document.querySelector('#apply-update');
      if (!button) {
        button = document.createElement('button');
        button.id = 'apply-update';
        button.type = 'button';
        button.className = 'secondary pwa-update';
        button.textContent = 'Update ready';
        document.body.append(button);
      }
      if (button) {
        button.hidden = false;
        button.onclick = () => {
          registration.waiting.postMessage({ type: 'APPLY_UPDATE' });
          button.textContent = 'Update active · reload when ready';
          button.onclick = () => location.reload();
        };
      }
    };
    registration.addEventListener('updatefound', () =>
      registration.installing?.addEventListener('statechange', showUpdate),
    );
    showUpdate();
    return registration;
  } catch {
    return null;
  }
}
export async function clearPwa(scope) {
  if ('clearAppBadge' in navigator) navigator.clearAppBadge().catch(() => {});
  navigator.serviceWorker?.controller?.postMessage({ type: 'CLEAR_SCOPE', scope });
  // Delete directly too; a worker may be stopped or an update may be waiting.
  if ('caches' in window) {
    await caches.delete(`todoer-page-${scope}`);
    await caches.delete('todoer-offline-meta-v1');
  }
}

// Browsers without an install prompt retain the platform instructions.
let installPrompt;
window.addEventListener('beforeinstallprompt', (event) => {
  const button = document.querySelector('[data-install-app]');
  if (!button) return;
  event.preventDefault();
  installPrompt = event;
  button.hidden = false;
  button.onclick = async () => {
    button.disabled = true;
    try {
      await installPrompt.prompt();
      await installPrompt.userChoice;
    } finally {
      installPrompt = null;
      button.hidden = true;
      button.disabled = false;
    }
  };
});
function installed() {
  document.querySelector('[data-install-app]')?.setAttribute('hidden', '');
  const status = document.querySelector('[data-install-status]');
  if (status) status.textContent = 'Todoer is installed and opens as its own app.';
}
window.addEventListener('appinstalled', installed);
if (matchMedia('(display-mode: standalone)').matches || navigator.standalone) installed();
