import { mountPage, unmountPage } from './page-lifecycle.js';

// Keep server URLs and full HTML responses as the navigation contract.
const modules = new Set([
  'site',
  'app',
  'task-page',
  'planning-page',
  'planning-workbench',
  'general-settings',
  'space-overview',
  'workspaces',
  'notification-picker',
  'capture-preferences',
  'account-security',
  'ai-billing',
  'push',
  'review',
  'admin-email',
  'delivery-settings',
]);
const appPath =
  /^\/(?:(?:inbox|today|upcoming|plan|recurring|review|trash|search|spaces|space|workspaces|lists|tasks|settings|notifications|admin)(?:\/|$)|$)/;
let pending;
let loadedPath = location.pathname;
let loadedURL = location.pathname + location.search;
const scrollPositions = new Map();
let scrollTimer;
let errorNotice;
let navigating = false;
const submitting = new WeakSet();
if (document.querySelector('#navigation')) history.scrollRestoration = 'manual';

function navigationError(nav, message, signin = false) {
  errorNotice?.remove();
  errorNotice = document.createElement('div');
  errorNotice.className = 'navigation-error';
  errorNotice.setAttribute('role', 'alert');
  const text = document.createElement('span');
  text.textContent = message;
  const retry = document.createElement(signin ? 'a' : 'button');
  retry.textContent = signin ? 'Sign in' : nav.form ? 'Reload saved data' : 'Try again';
  if (signin)
    retry.href =
      '/signin?next=' +
      encodeURIComponent(nav.destination.pathname + nav.destination.search);
  else {
    retry.type = 'button';
    retry.onclick = () =>
      navigate(nav.form ? nav.sourceURL : nav.destination, { restore: nav.restore });
  }
  const close = document.createElement('button');
  close.type = 'button';
  close.textContent = 'Dismiss';
  close.onclick = () => errorNotice.remove();
  errorNotice.append(text, retry, close);
  (document.querySelector('dialog[open]') || document.body).append(errorNotice);
}

function rememberScroll() {
  scrollPositions.set(location.href, [scrollX, scrollY]);
}
window.addEventListener(
  'scroll',
  () => {
    if (navigating) return;
    clearTimeout(scrollTimer);
    const url = location.href;
    const position = [scrollX, scrollY];
    scrollTimer = setTimeout(() => scrollPositions.set(url, position), 80);
  },
  { passive: true },
);

export function navigate(
  url,
  { replace = false, restore = false, preserveScroll = false, form, event } = {},
) {
  const destination = new URL(url, location.href);
  if (
    !window.htmx ||
    !document.querySelector('#navigation') ||
    destination.origin !== location.origin ||
    !appPath.test(destination.pathname)
  ) {
    location.assign(destination.href);
    return Promise.resolve();
  }
  if (destination.href === location.href && !restore) {
    replace = true;
    preserveScroll = true;
  }
  errorNotice?.remove();
  if (!restore) rememberScroll();
  clearTimeout(scrollTimer);
  navigating = true;
  pending?.abort();
  const abort = new AbortController();
  pending = abort;
  const nav = {
    destination,
    restore,
    preserveScroll,
    abort,
    form,
    submitter: event?.submitter,
    sourceURL: location.href,
  };
  return window.htmx
    .ajax(form ? 'POST' : 'GET', destination.href, {
      source: form || document.body,
      event,
      target: '#main',
      select: '#main',
      swap: 'outerHTML',
      push: restore || replace ? false : 'true',
      replace: replace ? 'true' : false,
      todoerNavigation: nav,
    })
    .then(() => nav.ready)
    .finally(() => {
      if (pending === abort) navigating = false;
    });
}

// Calendar controls change the rendered range without replacing the page.
export function replacePageURL(url) {
  history.replaceState({ ...history.state, htmx: true }, '', url);
  loadedPath = location.pathname;
  loadedURL = location.pathname + location.search;
}

export function refreshPage() {
  return navigate(location.href, { replace: true, preserveScroll: true });
}

// Listen after page controllers so task panels, filters and dialogs keep their flows.
window.addEventListener('click', (event) => {
  if (
    event.defaultPrevented ||
    event.button !== 0 ||
    event.metaKey ||
    event.ctrlKey ||
    event.shiftKey ||
    event.altKey
  )
    return;
  const link = event.target.closest?.('a[href]');
  if (
    !link ||
    link.download ||
    link.hasAttribute('download') ||
    (link.target && link.target !== '_self') ||
    link.closest('[data-full-navigation]')
  )
    return;
  const url = new URL(link.href, location.href);
  if (
    url.origin !== location.origin ||
    !appPath.test(url.pathname) ||
    !document.querySelector('#navigation') ||
    (url.pathname === location.pathname && url.search === location.search && url.hash)
  )
    return;
  event.preventDefault();
  navigate(url);
});

// Existing JS editors own their submits; ordinary forms keep their server handlers.
window.addEventListener('submit', (event) => {
  const form = event.target;
  const method = (
    event.submitter?.getAttribute('formmethod') || form.method
  ).toLowerCase();
  const target = event.submitter?.getAttribute('formtarget') || form.target;
  if (
    event.defaultPrevented ||
    !['get', 'post'].includes(method) ||
    (target && target !== '_self') ||
    form.closest('[data-full-navigation]') ||
    (method === 'post' &&
      (!form.hasAttribute('action') ||
        form.enctype !== 'application/x-www-form-urlencoded'))
  )
    return;
  const url = new URL(
    event.submitter?.getAttribute('formaction') || form.action,
    location.href,
  );
  if (
    !window.htmx ||
    url.origin !== location.origin ||
    !appPath.test(url.pathname) ||
    !document.querySelector('#navigation')
  )
    return;
  event.preventDefault();
  if (submitting.has(form)) return;
  if (method === 'get') {
    url.search = new URLSearchParams(new FormData(form, event.submitter)).toString();
    navigate(url);
  } else {
    navigate(url, { form, event });
  }
});

document.addEventListener('htmx:before:history:restore', (event) => {
  event.preventDefault();
  if (loadedURL === location.pathname + location.search) return;
  // Today owns local filter history so its open task panel remains mounted.
  if (
    loadedPath === location.pathname &&
    document.querySelector(
      '[data-planning-today]:not([data-workbench]):not([data-search-preview])',
    )
  )
    return;
  navigate(location.href, { restore: true });
});

document.addEventListener('htmx:before:request', (event) => {
  const ctx = event.detail.ctx;
  const nav = ctx?.todoerNavigation;
  if (!nav) return;
  if (nav.abort.signal.aborted) {
    event.preventDefault();
    return;
  }
  nav.abort.signal.addEventListener('abort', ctx.request.abort, { once: true });
  ctx.request.cache = 'no-store';
  if (nav.form) {
    submitting.add(nav.form);
    if (nav.submitter) nav.submitter.disabled = true;
  }
  document.querySelector('#main')?.setAttribute('aria-busy', 'true');
  nav.timer = setTimeout(
    () => document.documentElement.classList.add('page-loading'),
    180,
  );
});

document.addEventListener('htmx:after:request', (event) => {
  const ctx = event.detail.ctx;
  const nav = ctx?.todoerNavigation;
  if (!nav) return;
  if (nav.abort.signal.aborted) {
    event.preventDefault();
    return;
  }
  if (!ctx.response.raw.ok) {
    event.preventDefault();
    navigationError(
      nav,
      nav.form
        ? 'The save could not be confirmed. Check saved data before trying again.'
        : 'Could not open this page. Check your connection and try again.',
    );
    return;
  }
  const doc = new DOMParser().parseFromString(ctx.text, 'text/html');
  if (!doc.querySelector('#main') || !doc.querySelector('#navigation')) {
    event.preventDefault();
    if (new URL(ctx.response.raw.url).pathname === '/signin')
      navigationError(nav, 'Your session ended. Your drafts are kept.', true);
    else location.assign(ctx.response.raw.url);
    return;
  }
  const account = doc.querySelector('meta[name="account-id"]')?.content;
  if (account !== document.querySelector('meta[name="account-id"]')?.content) {
    event.preventDefault();
    location.assign(ctx.response.raw.url);
    return;
  }
  nav.document = doc;
  nav.scripts = [...doc.querySelectorAll('script[type="module"][src]')]
    .map((script) => new URL(script.getAttribute('src'), location.origin))
    .filter(
      (url) =>
        url.origin === location.origin &&
        modules.has(url.pathname.match(/^\/assets\/(.+)\.js$/)?.[1]),
    )
    .map((url) => url.pathname);
  // Controllers are imported explicitly once and mounted by page-lifecycle.
  doc.querySelectorAll('script').forEach((script) => script.remove());
  ctx.text = doc.documentElement.outerHTML;
});

function copyAttributes(current, next) {
  for (const attr of [...current.attributes]) current.removeAttribute(attr.name);
  for (const attr of next.attributes) current.setAttribute(attr.name, attr.value);
}

document.addEventListener('htmx:before:swap', (event) => {
  const ctx = event.detail.ctx;
  const nav = ctx?.todoerNavigation;
  if (!nav?.document) return;
  if (nav.abort.signal.aborted) {
    event.preventDefault();
    return;
  }
  const doc = nav.document;
  nav.sidebarScroll =
    document.querySelector('#navigation .sidebar-scroll')?.scrollTop || 0;
  unmountPage();
  for (const dialog of document.querySelectorAll('dialog[open]')) dialog.close();
  // Page dialogs can be appended beside #main. Their close listeners are disposed.
  for (const dialog of document.querySelectorAll('body > dialog')) dialog.remove();
  document.querySelectorAll('.list-settings-panel').forEach((panel) => panel.remove());
  const skip = document.querySelector('a.skip');
  const nextSkip = doc.querySelector('a.skip');
  if (skip && nextSkip) skip.replaceWith(nextSkip);
  const sidebar = document.querySelector('#navigation');
  const nextSidebar = doc.querySelector('#navigation');
  copyAttributes(sidebar, nextSidebar);
  sidebar.replaceChildren(...nextSidebar.childNodes);
  const header = document.querySelector('.mobile-header');
  const nextHeader = doc.querySelector('.mobile-header');
  if (header && nextHeader) header.replaceChildren(...nextHeader.childNodes);
  for (const selector of [
    '[data-notification-bell]',
    '[data-app-dialogs]',
    '.list-archive-banner',
  ]) {
    document.querySelector(selector)?.remove();
    const next = doc.querySelector(selector);
    if (next) document.body.append(next);
  }
  copyAttributes(document.body, doc.body);
  const csrf = document.querySelector('meta[name="csrf-token"]');
  const nextCsrf = doc.querySelector('meta[name="csrf-token"]');
  if (csrf && nextCsrf) csrf.content = nextCsrf.content;
  for (const link of doc.querySelectorAll('link[rel="stylesheet"]')) {
    const href = link.getAttribute('href');
    if (
      (href.startsWith('/assets/') || href === '/review.css') &&
      ![...document.querySelectorAll('link[rel="stylesheet"]')].some(
        (old) => old.getAttribute('href') === href,
      )
    )
      document.head.append(link.cloneNode());
  }
});

document.addEventListener('htmx:after:swap', (event) => {
  const nav = event.detail.ctx?.todoerNavigation;
  if (!nav?.document || nav.abort.signal.aborted) return;
  loadedPath = location.pathname;
  loadedURL = location.pathname + location.search;
  nav.ready = activatePage(nav).catch(() => {
    if (nav.abort.signal.aborted) return;
    navigationError(
      nav,
      'Some controls could not load. Check your connection and try again.',
    );
  });
});

async function activatePage(nav) {
  const main = document.querySelector('#main');
  main.inert = true;
  try {
    mountPage();
    for (const path of nav.scripts) await import(path);
  } finally {
    main.inert = false;
  }
  if (nav.abort.signal.aborted) return;
  document.dispatchEvent(new Event('todoer:page-ready'));
  const sidebar = document.querySelector('#navigation .sidebar-scroll');
  if (sidebar) sidebar.scrollTop = nav.sidebarScroll;
  main.removeAttribute('aria-busy');
  main.setAttribute('tabindex', '-1');
  main.focus({ preventScroll: true });
  const scroll =
    nav.restore || nav.preserveScroll ? scrollPositions.get(location.href) : [0, 0];
  window.scrollTo(...(scroll || [0, 0]));
  if (nav.destination.hash) {
    document
      .getElementById(decodeURIComponent(nav.destination.hash.slice(1)))
      ?.scrollIntoView();
  }
}

document.addEventListener('htmx:error', (event) => {
  const nav = event.detail.ctx?.todoerNavigation;
  if (nav && !nav.abort.signal.aborted)
    navigationError(
      nav,
      nav.form
        ? 'The save could not be confirmed. Check saved data before trying again.'
        : 'Could not open this page. Check your connection and try again.',
    );
});

document.addEventListener('htmx:finally:request', (event) => {
  const nav = event.detail.ctx?.todoerNavigation;
  if (!nav) return;
  clearTimeout(nav.timer);
  if (nav.form) {
    submitting.delete(nav.form);
    if (nav.submitter) nav.submitter.disabled = false;
  }
  if (pending === nav.abort) {
    document.documentElement.classList.remove('page-loading');
    document.querySelector('#main')?.removeAttribute('aria-busy');
  }
});
