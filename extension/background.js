import { browserApi, pinnedPath, settings, siteHost } from './client.js';

const { contextMenus, runtime, tabs } = browserApi;
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
  await tabs.create({ url: capture.href });
}

contextMenus.onClicked.addListener((info, tab) => {
  handleMenu(info, tab).catch(console.error);
});
