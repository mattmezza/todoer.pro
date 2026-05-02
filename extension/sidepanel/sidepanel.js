/**
 * Todoer.pro Extension — Side Panel Script
 *
 * Loads the full Todoer app in an iframe.
 * Provides back/forward/reload/open-in-tab controls.
 */

'use strict';

const DEFAULTS = {
  baseUrl: 'https://todoer.pro',
  theme: 'system',
};

async function loadSettings() {
  return new Promise((resolve) => {
    chrome.storage.sync.get(DEFAULTS, (items) => resolve(items));
  });
}

async function init() {
  const settings = await loadSettings();
  const base = (settings.baseUrl || 'https://todoer.pro').replace(/\/$/, '');
  const startUrl = `${base}/inbox`;

  const frame = document.getElementById('app-frame');
  const errorDiv = document.getElementById('panel-error');
  const errorUrl = document.getElementById('error-url');

  // Apply theme to panel chrome
  if (settings.theme === 'light') {
    document.documentElement.style.setProperty('--bg', '#f8fafc');
  }

  // Load the iframe
  frame.src = startUrl;
  errorUrl.textContent = startUrl;

  frame.addEventListener('error', () => {
    frame.style.display = 'none';
    errorDiv.style.display = '';
  });

  // Navigation buttons
  document.getElementById('btn-back').addEventListener('click', () => {
    try { frame.contentWindow.history.back(); } catch { }
  });

  document.getElementById('btn-forward').addEventListener('click', () => {
    try { frame.contentWindow.history.forward(); } catch { }
  });

  document.getElementById('btn-reload').addEventListener('click', () => {
    try {
      frame.contentWindow.location.reload();
    } catch {
      // Cross-origin: reset src
      const src = frame.src;
      frame.src = '';
      frame.src = src;
    }
  });

  document.getElementById('btn-newtab').addEventListener('click', () => {
    let url = startUrl;
    try {
      url = frame.contentWindow.location.href;
    } catch { /* cross-origin */ }
    chrome.tabs.create({ url });
  });

  document.getElementById('btn-open-in-tab').addEventListener('click', (e) => {
    e.preventDefault();
    chrome.tabs.create({ url: startUrl });
  });
}

document.addEventListener('DOMContentLoaded', init);
