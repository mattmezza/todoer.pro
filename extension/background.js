import {
  browserApi,
  listId,
  pinnedDestination,
  settings,
  siteHost,
  storage,
  supportsSidePanel,
} from './client.js';

const { action, contextMenus, runtime, sidePanel, sidebarAction, tabs } = browserApi;
const pagePatterns = ['http://*/*', 'https://*/*'];

function setPanelMode(mode) {
  return action.setPopup({
    popup: supportsSidePanel && mode === 'side-panel' ? '' : 'popup.html',
  });
}

function restorePanelMode() {
  storage
    .get('panelMode')
    .then(({ panelMode }) => setPanelMode(panelMode))
    .catch(console.error);
}

runtime.onStartup.addListener(restorePanelMode);
storage.onChanged.addListener((changes) => {
  if (changes.panelMode) setPanelMode(changes.panelMode.newValue).catch(console.error);
});
action.onClicked.addListener((tab) => {
  // Open during the toolbar gesture so activeTab covers the site that invoked Todoer.
  if (sidePanel?.open) sidePanel.open({ windowId: tab.windowId }).catch(console.error);
  else if (sidebarAction?.open) sidebarAction.open().catch(console.error);
});

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
      title: 'Choose Todoer link for this site',
      contexts: ['page', 'action'],
      documentUrlPatterns: pagePatterns,
    });
  });
  restorePanelMode();
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
  const list = listId(await pinnedDestination(origin, hostname));
  const capture = new URL('/capture', origin);
  if (list) capture.searchParams.set('list', list);
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
