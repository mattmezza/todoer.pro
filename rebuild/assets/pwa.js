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
      const button = document.querySelector('#apply-update');
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
  navigator.serviceWorker?.controller?.postMessage({ type: 'CLEAR_SCOPE', scope });
  // Delete directly too; a worker may be stopped or an update may be waiting.
  if ('caches' in window) {
    await caches.delete(`todoer-page-${scope}`);
    await caches.delete('todoer-offline-meta-v1');
  }
}
