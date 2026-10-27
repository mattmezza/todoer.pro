import { request as fetch } from './request.js';
// Device enablement is explicitly initiated by a user gesture.
export function mountPushSettings(root) {
  if (!root) return () => {};
  const status = root.querySelector('[data-push-status]');
  const enable = root.querySelector('[data-push-enable]');
  const disable = root.querySelector('[data-push-disable]');
  const optIn = root.querySelector('[data-push-opt-in]');
  const label = root.querySelector('[data-push-label]');
  const csrf = root.dataset.csrf;
  const abort = new AbortController();
  let registration;
  let current;
  let deviceId;
  let busy = false;
  let active = true;
  const say = (text) => {
    if (active) status.textContent = text;
  };
  async function request(path, method = 'GET', body) {
    const r = await fetch(path, {
      method,
      credentials: 'same-origin',
      signal: abort.signal,
      headers: { 'Content-Type': 'application/json', 'x-csrf-token': csrf },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    if (!r.ok) {
      const error = await r.json().catch(() => ({}));
      throw new Error(
        error.error?.message ||
          error.message ||
          `Request failed (${r.status}). Reload and try again.`,
      );
    }
    return r.status === 204 ? null : r.json();
  }
  const fromBase64 = (s) =>
    Uint8Array.from(
      atob(
        s.replace(/-/g, '+').replace(/_/g, '/') + '='.repeat((4 - (s.length % 4)) % 4),
      ),
      (c) => c.charCodeAt(0),
    );
  async function enableDevice() {
    if (busy) return;
    busy = true;
    enable.disabled = true;
    try {
      // subscribe itself requests permission in the direct click path. Avoid an
      // asynchronous fetch before this browser permission operation.
      current = await registration.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: fromBase64(root.dataset.publicKey),
      });
      const serialized = current.toJSON();
      const device = await request('/api/v1/push/devices', 'POST', {
        endpoint: serialized.endpoint,
        keys: serialized.keys,
        label: label.value.trim() || 'This browser',
      });
      deviceId = device.id;
      optIn.checked = true;
      disable.hidden = false;
      say('Push reminders enabled for this login session.');
    } catch (e) {
      if (e.name !== 'AbortError')
        say(
          `Could not enable reminders: ${e.message}. If browser permission was granted, retry this same button to complete registration.`,
        );
    } finally {
      busy = false;
      enable.disabled = false;
    }
  }
  async function disableDevice() {
    if (busy) return;
    busy = true;
    disable.disabled = true;
    try {
      // Server revocation first, so failed browser cleanup cannot keep delivery on.
      if (deviceId)
        await request(`/api/v1/push/devices/${encodeURIComponent(deviceId)}`, 'DELETE');
      else {
        // No matching registration belongs to this account; browser cleanup is sufficient.
        // An existing registration for another account remains session-bound there.
      }
      if (current && !(await current.unsubscribe()))
        throw new Error('Browser unsubscribe failed; server delivery has stopped');
      current = null;
      deviceId = null;
      disable.hidden = true;
      say('This browser is unsubscribed. Other devices can be managed below.');
    } catch (e) {
      if (e.name !== 'AbortError') say(e.message);
    } finally {
      busy = false;
      disable.disabled = false;
    }
  }
  async function changeOptIn() {
    const value = optIn.checked;
    optIn.disabled = true;
    try {
      await request('/api/v1/push/preferences', 'PUT', { opt_in: value });
      say(
        value
          ? 'Push reminders allowed on active devices.'
          : 'Push reminders disabled for all your devices.',
      );
    } catch (e) {
      optIn.checked = !value;
      if (e.name !== 'AbortError') say(e.message);
    } finally {
      optIn.disabled = false;
    }
  }
  enable.addEventListener('click', enableDevice);
  disable.addEventListener('click', disableDevice);
  optIn.addEventListener('change', changeOptIn);
  (async () => {
    if (
      !isSecureContext ||
      !('serviceWorker' in navigator) ||
      !('PushManager' in window) ||
      !('Notification' in window)
    ) {
      say(
        'This browser cannot enable Web Push here. Use HTTPS and a supported browser. On iPhone or iPad, open the Home Screen app.',
      );
      return;
    }
    if (Notification.permission === 'denied') {
      say(
        'Notifications are blocked in browser settings. Allow them there, then reload.',
      );
      return;
    }
    try {
      const config = await request('/api/v1/push/key');
      if (!config.available || !config.public_key) {
        say('Server push delivery is not configured.');
        return;
      }
      // Do not leave the enable button waiting forever for an absent service worker.
      registration = await navigator.serviceWorker.register('/sw.js', { scope: '/' });
      if (!registration.active) {
        registration = await Promise.race([
          navigator.serviceWorker.ready,
          new Promise((_, reject) =>
            setTimeout(
              () =>
                reject(
                  new Error('Service worker activation timed out; reload and try again'),
                ),
              8000,
            ),
          ),
        ]);
      }
      if (!registration?.active) {
        say(
          'The Todoer service worker is not active yet. Reload once the app has finished loading.',
        );
        return;
      }
      root.dataset.publicKey = config.public_key;
      current = await registration.pushManager.getSubscription();
      if (current) {
        const digest = await crypto.subtle.digest(
          'SHA-256',
          new TextEncoder().encode(current.endpoint),
        );
        const hash = Array.from(new Uint8Array(digest), (b) =>
          b.toString(16).padStart(2, '0'),
        ).join('');
        const settings = await request('/api/v1/push/devices');
        deviceId = settings.devices.find((d) => d.endpoint_hash === hash)?.id;
      }
      enable.hidden = false;
      disable.hidden = !current;
      say(
        current
          ? 'This browser has a push subscription. Enable to bind it to this login session, or disable it.'
          : 'Enable reminders to request browser notification permission.',
      );
    } catch (e) {
      if (e.name !== 'AbortError') say(`Push status unavailable: ${e.message}`);
    }
  })();
  return () => {
    active = false;
    abort.abort();
    enable.removeEventListener('click', enableDevice);
    disable.removeEventListener('click', disableDevice);
    optIn.removeEventListener('change', changeOptIn);
  };
}
const root = document.querySelector('[data-push-settings]');
if (root) mountPushSettings(root);
