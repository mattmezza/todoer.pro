/**
 * Todoer.pro Extension — Background Service Worker (Manifest V3)
 *
 * Responsibilities:
 * - Initialize context menu entries
 * - Handle context menu clicks (quick-add with selected text / page URL)
 * - Handle the "open-side-panel" keyboard shortcut
 * - Open the side panel on the extension icon click (alongside popup)
 */

// ─── Defaults ────────────────────────────────────────────────────────────────

const DEFAULTS = {
  baseUrl: 'https://todoer.pro',
  defaultList: '',
  contextMenuEnabled: true,
  theme: 'system',
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

function getSettings() {
  return new Promise((resolve) => {
    chrome.storage.sync.get(DEFAULTS, (items) => resolve(items));
  });
}

// ─── Context Menu ─────────────────────────────────────────────────────────────

function buildContextMenu() {
  chrome.contextMenus.removeAll(() => {
    getSettings().then((settings) => {
      if (!settings.contextMenuEnabled) return;

      chrome.contextMenus.create({
        id: 'todoer-add-task',
        title: 'Add to Todoer',
        contexts: ['all'],
      });

      chrome.contextMenus.create({
        id: 'todoer-add-with-selection',
        title: 'Add "%s" to Todoer',
        contexts: ['selection'],
      });
    });
  });
}

// Build menu on install and startup
chrome.runtime.onInstalled.addListener(() => {
  buildContextMenu();
  // Store defaults on first install
  chrome.storage.sync.get(DEFAULTS, (items) => {
    chrome.storage.sync.set(items);
  });
});

chrome.runtime.onStartup.addListener(buildContextMenu);

// Rebuild menu when settings change (e.g. contextMenuEnabled toggled)
chrome.storage.onChanged.addListener((changes) => {
  if ('contextMenuEnabled' in changes || 'baseUrl' in changes) {
    buildContextMenu();
  }
});

// ─── Context Menu Click ───────────────────────────────────────────────────────

chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (
    info.menuItemId !== 'todoer-add-task' &&
    info.menuItemId !== 'todoer-add-with-selection'
  ) {
    return;
  }

  const title =
    info.menuItemId === 'todoer-add-with-selection' && info.selectionText
      ? info.selectionText.trim().slice(0, 200)
      : tab.title || '';

  const descr = tab.url || '';

  // Store pending quick-add data, then open the popup
  chrome.storage.session.set(
    { quickAddPrefill: { title, descr } },
    () => {
      // Open the popup programmatically — requires "activeTab" permission
      // Fallback: open a small window with the popup page
      chrome.windows.create({
        url: chrome.runtime.getURL(
          `popup/popup.html?prefill=1`
        ),
        type: 'popup',
        width: 420,
        height: 560,
        focused: true,
      });
    }
  );
});

// ─── Keyboard Shortcut — Side Panel ──────────────────────────────────────────

chrome.commands.onCommand.addListener((command) => {
  if (command === 'open-side-panel') {
    chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
      if (tabs[0]) {
        chrome.sidePanel.open({ tabId: tabs[0].id });
      }
    });
  }
});

// ─── Side Panel — open on icon click (alongside popup) ───────────────────────
// The popup is shown because manifest declares default_popup.
// We also enable the side panel so users can pin it open.
chrome.sidePanel
  .setPanelBehavior({ openPanelOnActionClick: false })
  .catch(() => {
    // Ignore if API not available in this build
  });
