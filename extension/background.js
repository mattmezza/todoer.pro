import {
  browserApi,
  listId,
  pinnedDestination,
  settings,
  siteHost,
  storage,
  supportsSidePanel,
  supportsSidePanelToggle,
  toolbarDestination,
} from './client.js';

const { action, contextMenus, runtime, sidePanel, sidebarAction, tabs } = browserApi;
const pagePatterns = ['http://*/*', 'https://*/*'];
const panelKey = (windowId) => `sidePanelOpen:${windowId}`;
const session = browserApi.storage.session;

let toggleSidePanel = false;

async function restorePanelMode() {
  const { panelMode, sidePanelToggle } = await storage.get([
    'panelMode',
    'sidePanelToggle',
  ]);
  const useSidePanel = supportsSidePanel && panelMode === 'side-panel';
  toggleSidePanel = useSidePanel && supportsSidePanelToggle && sidePanelToggle === true;
  await action.setPopup({ popup: useSidePanel ? '' : 'popup.html' });
}

const refreshPanelMode = () => restorePanelMode().catch(console.error);
refreshPanelMode();
runtime.onStartup.addListener(refreshPanelMode);
storage.onChanged.addListener((changes) => {
  if (changes.panelMode || changes.sidePanelToggle) refreshPanelMode();
});

sidePanel?.onOpened?.addListener(({ windowId }) => {
  session.set({ [panelKey(windowId)]: true }).catch(console.error);
});
sidePanel?.onClosed?.addListener(({ windowId }) => {
  session.remove(panelKey(windowId)).catch(console.error);
});
browserApi.windows.onRemoved.addListener((windowId) => {
  if (sidePanel?.onClosed) session.remove(panelKey(windowId)).catch(console.error);
});

async function openSidePanel(tab) {
  // Open during the toolbar gesture so activeTab covers the site that invoked Todoer.
  if (sidePanel?.open) {
    const window = { windowId: tab.windowId };
    if (!supportsSidePanelToggle) return sidePanel.open(window);
    const key = panelKey(tab.windowId);
    // Chromium requires open() before the toolbar gesture ends. If the
    // panel was already open, open() leaves it in place until we close it.
    const [previous, { sidePanelToggle }] = await Promise.all([
      session.get(key),
      storage.get('sidePanelToggle'),
      sidePanel.open(window),
    ]);
    if (previous[key] && sidePanelToggle) await sidePanel.close(window);
    return;
  }
  if (toggleSidePanel) return sidebarAction?.toggle();
  return sidebarAction?.open();
}

action.onClicked.addListener((tab) => {
  openSidePanel(tab).catch(console.error);
});

runtime.onInstalled.addListener(() => {
  contextMenus.removeAll(() => {
    contextMenus.create({ id: 'open-tab', title: 'Open in tab', contexts: ['action'] });
    contextMenus.create({
      id: 'settings',
      title: 'Extension settings',
      contexts: ['action'],
    });
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
  refreshPanelMode();
  settings().catch(console.error);
});

async function handleMenu(info, tab) {
  const source = info.pageUrl ?? tab?.url;
  if (info.menuItemId === 'settings') {
    await runtime.openOptionsPage();
    return;
  }
  if (info.menuItemId === 'open-tab') {
    const destination = await toolbarDestination(source);
    await tabs.create({ url: destination.href });
    return;
  }
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
