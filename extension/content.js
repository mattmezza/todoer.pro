/**
 * Todoer.pro Extension — Content Script
 *
 * Injected into every page. Currently minimal — only listens for messages
 * from the extension (e.g. to get selected text or page meta).
 */

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message.type === 'GET_PAGE_INFO') {
    sendResponse({
      title: document.title || '',
      url: location.href,
      selectedText: window.getSelection()?.toString()?.trim() || '',
    });
    return true; // keep channel open for async
  }
});
