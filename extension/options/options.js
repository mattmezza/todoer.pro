/**
 * Todoer.pro Extension — Options Page Script
 */

'use strict';

const DEFAULTS = {
  baseUrl: 'https://todoer.pro',
  defaultList: '',
  contextMenuEnabled: true,
  theme: 'system',
};

function applyTheme(theme) {
  const html = document.documentElement;
  if (theme === 'dark') {
    html.setAttribute('data-theme', 'dark');
  } else if (theme === 'light') {
    html.setAttribute('data-theme', 'light');
  } else {
    html.removeAttribute('data-theme');
  }
}

async function loadSettings() {
  return new Promise((resolve) => {
    chrome.storage.sync.get(DEFAULTS, (items) => resolve(items));
  });
}

async function saveSettings(settings) {
  return new Promise((resolve) => {
    chrome.storage.sync.set(settings, resolve);
  });
}

function showSaved() {
  const el = document.getElementById('save-status');
  el.classList.add('show');
  setTimeout(() => el.classList.remove('show'), 2000);
}

async function init() {
  const settings = await loadSettings();

  // Apply theme
  applyTheme(settings.theme);

  // Populate fields
  document.getElementById('base-url').value = settings.baseUrl || 'https://todoer.pro';
  document.getElementById('default-list').value = settings.defaultList || '';
  document.getElementById('context-menu-enabled').checked = settings.contextMenuEnabled !== false;
  document.getElementById('theme-select').value = settings.theme || 'system';

  // Update tip URL display
  updateUrlDisplay(settings.baseUrl);

  // Theme preview on change
  document.getElementById('theme-select').addEventListener('change', (e) => {
    applyTheme(e.target.value);
  });

  // Update URL display as user types
  document.getElementById('base-url').addEventListener('input', (e) => {
    updateUrlDisplay(e.target.value);
  });

  // Save button
  document.getElementById('btn-save').addEventListener('click', async () => {
    const rawUrl = document.getElementById('base-url').value.trim();
    // Normalize URL — ensure it starts with https:// or http://
    let baseUrl = rawUrl;
    if (baseUrl && !baseUrl.startsWith('http')) {
      baseUrl = 'https://' + baseUrl;
    }
    baseUrl = baseUrl.replace(/\/$/, '');

    const newSettings = {
      baseUrl,
      defaultList: document.getElementById('default-list').value.trim(),
      contextMenuEnabled: document.getElementById('context-menu-enabled').checked,
      theme: document.getElementById('theme-select').value,
    };

    await saveSettings(newSettings);
    document.getElementById('base-url').value = baseUrl;
    updateUrlDisplay(baseUrl);
    showSaved();
  });

  // Allow Ctrl+S / Cmd+S to save
  document.addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === 's') {
      e.preventDefault();
      document.getElementById('btn-save').click();
    }
  });
}

function updateUrlDisplay(url) {
  const display = document.getElementById('base-url-display');
  if (!display) return;
  try {
    const parsed = new URL(url.startsWith('http') ? url : 'https://' + url);
    display.textContent = parsed.hostname;
  } catch {
    display.textContent = url || 'todoer.pro';
  }
}

document.addEventListener('DOMContentLoaded', init);
