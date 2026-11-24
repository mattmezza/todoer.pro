import { browserApi, toolbarDestination } from './client.js';

const { permissions, storage, tabs } = browserApi;
const $ = (id) => document.getElementById(id);
const app = $('app');
const status = $('status');
const allow = $('allow');
const retry = $('retry');
const signIn = $('sign-in');
document.body.classList.toggle(
  'side-panel',
  new URLSearchParams(location.search).has('side'),
);

// A successful fetch can use different cookie rules from the embedded page.
function verifyFrame(url) {
  return new Promise((resolve) => {
    const finish = (ready) => {
      clearTimeout(timer);
      window.removeEventListener('message', onMessage);
      resolve(ready);
    };
    const onMessage = (event) => {
      if (
        event.source === app.contentWindow &&
        event.origin === url.origin &&
        event.data === 'todoer-panel-ready'
      ) {
        finish(true);
      }
    };
    const timer = setTimeout(() => finish(false), 3500);
    window.addEventListener('message', onMessage);
    app.src = url.href;
  });
}

async function connect(destination, canRetry = true) {
  allow.hidden = true;
  retry.hidden = true;
  status.textContent = 'Loading Todoer…';
  const panelUrl = new URL('/api/extension/panel', destination);
  const response = await fetch(panelUrl, {
    method: 'POST',
    headers: { 'X-Todoer-Browser-Panel': '1' },
    credentials: 'include',
    redirect: 'error',
    signal: AbortSignal.timeout(10000),
  });
  if (response.status === 204) {
    if (
      response.headers.get('X-Todoer-Browser-Panel') === 'verify-frame' &&
      !(await verifyFrame(panelUrl))
    ) {
      if (canRetry) return connect(destination, false);
      status.textContent =
        'Todoer could not start the session in this panel. Open Todoer in a tab from the toolbar icon’s right-click menu, then retry.';
      retry.hidden = false;
      return;
    }
    app.allow = `clipboard-write ${destination.origin}`;
    app.src = destination.href;
    app.hidden = false;
    $('setup').hidden = true;
  } else if (response.status === 401) {
    const login = new URL('/signin', destination);
    login.searchParams.set(
      'next',
      destination.pathname + destination.search + destination.hash,
    );
    signIn.href = login.href;
    signIn.hidden = false;
    status.textContent = 'Sign in, then reopen this panel.';
  } else if ([404, 405].includes(response.status)) {
    status.textContent = 'This Todoer server needs an update to open in the toolbar.';
  } else {
    status.textContent = 'Todoer could not connect to the toolbar panel.';
  }
}

function unavailable() {
  status.textContent = 'Could not reach the Todoer server.';
  retry.hidden = false;
}

retry.addEventListener('click', () => location.reload());
storage.onChanged.addListener((changes) => {
  if (changes.server) location.reload();
});

try {
  const [tab] = await tabs.query({ active: true, currentWindow: true });
  const destination = await toolbarDestination(tab?.url);

  const { protocol, hostname } = destination;
  const hostAccess = { origins: [`${protocol}//${hostname}/*`] };
  if (await permissions.contains(hostAccess)) {
    await connect(destination);
  } else {
    status.textContent = `Allow the extension to open Todoer from ${destination.host}.`;
    allow.hidden = false;
    allow.addEventListener('click', async () => {
      try {
        // Start the frame from a document created with the new host access.
        if (await permissions.request(hostAccess)) location.reload();
      } catch {
        unavailable();
      }
    });
  }
} catch {
  unavailable();
}
