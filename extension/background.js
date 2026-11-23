import { browserApi, pinPrefix, settings, siteHost, storage } from './client.js';

const { action, contextMenus, runtime, tabs, windows } = browserApi;
const openWindows = browserApi.storage.session;
const pagePatterns = ['http://*/*', 'https://*/*'];

runtime.onInstalled.addListener(() => {
  contextMenus.removeAll(() => {
    contextMenus.create({
      id: 'capture-page',
      title: 'Capture this page in Todoer',
      contexts: ['page', 'selection', 'action'],
      documentUrlPatterns: pagePatterns,
    });
    contextMenus.create({
      id: 'site-pin',
      title: 'Choose Todoer list for this site',
      contexts: ['page', 'action'],
      documentUrlPatterns: pagePatterns,
    });
  });
  settings().catch(console.error);
});

async function pinnedPath(origin, hostname) {
  if (!hostname) return undefined;
  const key = pinPrefix(origin) + hostname;
  const path = (await storage.get(key))[key];
  if (typeof path === 'string' && /^\/lists\/[a-z0-9_-]+$/i.test(path)) return path;
}

function narrowWindow(url) {
  return windows.create({ url, type: 'popup', focused: true, width: 480, height: 760 });
}

async function openTodoer(tab) {
  const origin = await settings();
  const path = await pinnedPath(origin, siteHost(tab.url));
  const url = new URL(path ?? '/inbox', origin);
  if (path) url.searchParams.set('browser', '1');

  // Keep an existing window's current page and any unsaved edits intact.
  await navigator.locks.request(url.href, async () => {
    const key = 'window:' + url.href;
    const id = (await openWindows.get(key))[key];
    if (id !== undefined) {
      try {
        await windows.update(id, { focused: true });
        return;
      } catch {
        // The previous window was closed.
      }
    }
    const opened = await narrowWindow(url.href);
    if (opened.id !== undefined) await openWindows.set({ [key]: opened.id });
  });
}

windows.onRemoved.addListener(async (id) => {
  const saved = await openWindows.get(null);
  await openWindows.remove(Object.keys(saved).filter((key) => saved[key] === id));
});

async function handleMenu(info, tab) {
  const source = info.pageUrl ?? tab?.url;
  const hostname = siteHost(source);
  if (!hostname) return;
  if (info.menuItemId === 'site-pin') {
    const options = new URL(runtime.getURL('options.html'));
    options.hash = new URLSearchParams({ site: hostname });
    await tabs.create({ url: options.href });
    return;
  }

  if (info.menuItemId !== 'capture-page') return;
  const origin = await settings();
  const path = await pinnedPath(origin, hostname);
  const capture = new URL('/capture', origin);
  if (path) capture.searchParams.set('list', path.slice('/lists/'.length));
  const page = new URL(source);
  page.username = '';
  page.password = '';
  const fragment = new URLSearchParams({
    title: (tab?.title ?? '').trim().replace(/\s+/g, ' ').slice(0, 500) || hostname,
    url: page.href.length <= 8000 ? page.href : '',
    selection: (info.selectionText ?? '').trim().slice(0, 5000),
  });
  // Draft text stays in the fragment until the Todoer page reads it locally.
  capture.hash = 'todoer-capture&' + fragment;
  await narrowWindow(capture.href);
}

action.onClicked.addListener((tab) => {
  openTodoer(tab).catch(console.error);
});
contextMenus.onClicked.addListener((info, tab) => {
  handleMenu(info, tab).catch(console.error);
});
