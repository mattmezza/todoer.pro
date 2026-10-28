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

// Installation is a device-level suggestion, rather than an account notification.
const installNoticeKey = 'todoer-install-notice-v1';
const standalone = () =>
  matchMedia('(display-mode: standalone)').matches || navigator.standalone;
const mobileDevice = () =>
  navigator.userAgentData?.mobile ||
  /Android|iPhone|iPad|iPod/.test(navigator.userAgent) ||
  (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
let installPrompt;
let installNotice;

async function promptInstall(button) {
  if (!installPrompt) return;
  button.disabled = true;
  try {
    await installPrompt.prompt();
    await installPrompt.userChoice;
    installNotice?.remove();
  } catch {
    const status = document.querySelector('[data-install-status]');
    if (status)
      status.textContent =
        'Use your browser’s Add to Home Screen option to install Todoer.';
  } finally {
    installPrompt = null;
    document.querySelectorAll('[data-install-app]').forEach((el) => {
      el.hidden = true;
    });
    button.disabled = false;
  }
}
function wireInstallButtons() {
  document.querySelectorAll('[data-install-app]').forEach((button) => {
    button.hidden = !installPrompt;
    button.onclick = () => promptInstall(button);
  });
}
window.addEventListener('beforeinstallprompt', (event) => {
  event.preventDefault();
  installPrompt = event;
  wireInstallButtons();
});
function installed() {
  installNotice?.remove();
  installPrompt = null;
  wireInstallButtons();
  const status = document.querySelector('[data-install-status]');
  if (status) status.textContent = 'Todoer is installed and opens as its own app.';
}
window.addEventListener('appinstalled', installed);
if (standalone()) installed();

function suggestInstall() {
  if (
    !isSecureContext ||
    standalone() ||
    !mobileDevice() ||
    !document.querySelector('meta[name="account-id"]')?.content
  )
    return;
  try {
    if (localStorage.getItem(installNoticeKey)) return;
    localStorage.setItem(installNoticeKey, 'seen');
  } catch {
    // Avoid a recurring prompt when storage is unavailable.
    return;
  }
  installNotice = document.createElement('section');
  installNotice.className = 'pwa-install-notice';
  installNotice.setAttribute('role', 'status');
  installNotice.setAttribute('aria-label', 'Install Todoer');
  const logo = document.createElement('img');
  logo.src = '/assets/icon192.png';
  logo.alt = '';
  logo.width = logo.height = 40;
  const text = document.createElement('div');
  const heading = document.createElement('strong');
  heading.textContent = 'Keep Todoer close';
  const description = document.createElement('p');
  const apple =
    /iPhone|iPad|iPod/.test(navigator.userAgent) ||
    (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
  description.textContent = apple
    ? 'Choose Share → Add to Home Screen, then open Todoer from its icon. You can enable push notifications there too.'
    : 'Install Todoer for quick access from your Home Screen. Use Install app or Add to Home Screen in your browser menu.';
  const action = document.createElement('button');
  action.type = 'button';
  action.className = 'primary';
  action.dataset.installApp = '';
  action.textContent = 'Install Todoer';
  action.hidden = true;
  const close = document.createElement('button');
  close.type = 'button';
  close.className = 'pwa-install-dismiss';
  close.setAttribute('aria-label', 'Dismiss installation suggestion');
  close.textContent = '×';
  close.onclick = () => installNotice.remove();
  text.append(heading, description, action);
  installNotice.append(logo, text, close);
  document.body.append(installNotice);
  wireInstallButtons();
}
suggestInstall();
