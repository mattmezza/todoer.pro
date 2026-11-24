import { browserApi, pinnedPath, settings, siteHost } from './client.js';

const { permissions, tabs } = browserApi;
const $ = (id) => document.getElementById(id);
const status = $('status');
const allow = $('allow');
const signIn = $('sign-in');

async function connect(destination) {
  allow.hidden = true;
  status.textContent = 'Loading Todoer…';
  const response = await fetch(new URL('/api/extension/panel', destination), {
    method: 'POST',
    headers: { 'X-Todoer-Browser-Panel': '1' },
    credentials: 'include',
    redirect: 'error',
    signal: AbortSignal.timeout(10000),
  });
  if (response.status === 204) {
    $('app').src = destination.href;
    $('app').hidden = false;
    $('setup').hidden = true;
  } else if (response.status === 401) {
    const login = new URL('/signin', destination);
    login.searchParams.set('next', destination.pathname + destination.search);
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
}

try {
  const [origin, [tab]] = await Promise.all([
    settings(),
    tabs.query({ active: true, currentWindow: true }),
  ]);
  const path = await pinnedPath(origin, siteHost(tab?.url));
  const destination = new URL(path ?? '/inbox', origin);
  if (path) destination.searchParams.set('browser', '1');
  $('open-tab').href = destination.href;
  $('links').hidden = false;

  const { protocol, hostname } = destination;
  const hostAccess = { origins: [`${protocol}//${hostname}/*`] };
  if (await permissions.contains(hostAccess)) {
    await connect(destination);
  } else {
    status.textContent = `Allow the extension to open Todoer from ${destination.host}.`;
    allow.hidden = false;
    allow.addEventListener('click', async () => {
      try {
        if (await permissions.request(hostAccess)) await connect(destination);
      } catch {
        unavailable();
      }
    });
  }
} catch {
  unavailable();
}
