/**
 * Todoer.pro Extension — Popup Script
 *
 * Handles:
 * - Auth state detection (ping /inbox; redirect to /signin if 401/redirect)
 * - Quick-add task form (POST /tasks with form-encoded data)
 * - Live search (GET /search?q=...)
 * - Browse / navigation links
 * - Pre-fill from context-menu action (chrome.storage.session)
 * - Theme application
 */

'use strict';

// ─── Settings ─────────────────────────────────────────────────────────────────

const DEFAULTS = {
  baseUrl: 'https://todoer.pro',
  defaultList: '',
  contextMenuEnabled: true,
  theme: 'system',
};

let settings = { ...DEFAULTS };

async function loadSettings() {
  return new Promise((resolve) => {
    chrome.storage.sync.get(DEFAULTS, (items) => {
      settings = items;
      resolve(items);
    });
  });
}

function baseUrl() {
  return (settings.baseUrl || 'https://todoer.pro').replace(/\/$/, '');
}

// ─── Theme ────────────────────────────────────────────────────────────────────

function applyTheme(theme) {
  const html = document.documentElement;
  if (theme === 'dark') {
    html.setAttribute('data-theme', 'dark');
  } else if (theme === 'light') {
    html.setAttribute('data-theme', 'light');
  } else {
    html.removeAttribute('data-theme'); // system default via media query
  }
}

// ─── Toast ────────────────────────────────────────────────────────────────────

let toastTimer = null;

function showToast(msg, type = '') {
  const el = document.getElementById('toast');
  el.textContent = msg;
  el.className = 'toast show ' + type;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    el.className = 'toast';
  }, 2800);
}

// ─── Status bar ───────────────────────────────────────────────────────────────

function setStatus(text, online = null) {
  document.getElementById('status-text').textContent = text;
  const dot = document.getElementById('status-dot');
  dot.className = 'status-dot' + (online === true ? ' online' : online === false ? ' offline' : '');
}

// ─── Auth check ───────────────────────────────────────────────────────────────

/**
 * Check auth by hitting /inbox (a page that requires login).
 * If the server redirects to /signin, the user is not authenticated.
 * We use fetch with redirect:'manual' so we can detect the redirect.
 */
async function checkAuth() {
  try {
    const res = await fetch(`${baseUrl()}/inbox`, {
      method: 'GET',
      credentials: 'include',
      redirect: 'manual',
    });
    // 200 = logged in; 0 (opaque redirect) or 3xx = not logged in
    if (res.status === 200) {
      return true;
    }
    if (res.type === 'opaqueredirect' || res.status === 303 || res.status === 302 || res.status === 301) {
      return false;
    }
    // 401, 403 → not auth
    if (res.status === 401 || res.status === 403) {
      return false;
    }
    return true; // assume logged in for other statuses
  } catch {
    return null; // network error
  }
}

// ─── Spaces / Lists / Sections data ──────────────────────────────────────────

let allSpaces = [];
let allListsBySpace = {};
let allSectionsByList = {};

async function fetchJson(path) {
  // The app is server-rendered HTML, not a JSON API.
  // We use the quick-add endpoint's dynamic selects to populate data.
  // We parse the HTML to extract option values.
  const res = await fetch(`${baseUrl()}${path}`, {
    credentials: 'include',
    redirect: 'manual',
    headers: { 'Accept': 'text/html' },
  });
  if (!res.ok && res.status !== 0) throw new Error(`HTTP ${res.status}`);
  return res.text();
}

/**
 * Parse <select name="..."> options from HTML fragment.
 * Returns array of { value, label, selected }
 */
function parseOptions(html, selectName) {
  const parser = new DOMParser();
  const doc = parser.parseFromString(html, 'text/html');
  const sel = doc.querySelector(`select[name="${selectName}"]`);
  if (!sel) return [];
  return Array.from(sel.options).map((o) => ({
    value: o.value,
    label: o.text.trim(),
    selected: o.defaultSelected || o.selected,
  }));
}

/**
 * Load the quick-add form HTML and populate local data structures.
 */
async function loadQuickAddData() {
  try {
    const html = await fetchJson('/tasks/quick-add');
    const parser = new DOMParser();
    const doc = parser.parseFromString(html, 'text/html');

    // Spaces
    const spaceOpts = parseOptions(html, 'space');
    allSpaces = spaceOpts;
    populateSelect('space-select', spaceOpts);

    const defaultSpaceOpt = spaceOpts.find((o) => o.selected) || spaceOpts[0];
    if (!defaultSpaceOpt) return;

    // Lists
    const listOpts = parseOptions(html, 'list');
    const defaultListOpt = listOpts.find((o) => o.selected) || listOpts[0];
    populateSelect('list-select', listOpts);

    // Sections
    const sectionOpts = parseOptions(html, 'section');
    populateSelect('section-select', sectionOpts);

    // Cache
    allListsBySpace[defaultSpaceOpt.value] = listOpts;
    if (defaultListOpt) {
      allSectionsByList[defaultListOpt.value] = sectionOpts;
    }
  } catch (err) {
    console.warn('Failed to load quick-add data:', err);
  }
}

function populateSelect(id, options) {
  const sel = document.getElementById(id);
  sel.innerHTML = '';
  if (options.length === 0) {
    sel.innerHTML = '<option value="">None</option>';
    return;
  }
  for (const opt of options) {
    const el = document.createElement('option');
    el.value = opt.value;
    el.textContent = opt.label;
    if (opt.selected) el.selected = true;
    sel.appendChild(el);
  }
}

/**
 * When space changes, fetch lists for that space.
 */
async function onSpaceChange(spaceValue) {
  if (allListsBySpace[spaceValue]) {
    populateSelect('list-select', allListsBySpace[spaceValue]);
    const firstList = allListsBySpace[spaceValue][0];
    if (firstList) await onListChange(firstList.value);
    return;
  }
  try {
    const html = await fetchJson(`/tasks/quick-add/lists?space=${encodeURIComponent(spaceValue)}`);
    const listOpts = parseOptions(html, 'list');
    allListsBySpace[spaceValue] = listOpts;
    populateSelect('list-select', listOpts);
    const defaultList = listOpts.find((o) => o.selected) || listOpts[0];
    if (defaultList) await onListChange(defaultList.value);
  } catch (err) {
    console.warn('Failed to load lists:', err);
  }
}

/**
 * When list changes, fetch sections for that list.
 */
async function onListChange(listValue) {
  if (allSectionsByList[listValue]) {
    populateSelect('section-select', allSectionsByList[listValue]);
    return;
  }
  try {
    const html = await fetchJson(`/tasks/quick-add/sections?list=${encodeURIComponent(listValue)}`);
    const sectionOpts = parseOptions(html, 'section');
    allSectionsByList[listValue] = sectionOpts;
    populateSelect('section-select', sectionOpts);
  } catch (err) {
    console.warn('Failed to load sections:', err);
  }
}

// ─── Add Task Form ────────────────────────────────────────────────────────────

async function submitTask(form) {
  const title = form.querySelector('#task-title').value.trim();
  const descr = form.querySelector('#task-descr').value.trim();
  const section = form.querySelector('#section-select').value;

  if (!title) {
    form.querySelector('#task-title').focus();
    return;
  }
  if (!section) {
    showToast('Select a section first', 'error');
    return;
  }

  const submitBtn = document.getElementById('btn-submit');
  const label = document.getElementById('submit-label');
  const spinner = document.getElementById('submit-spinner');
  submitBtn.disabled = true;
  label.style.display = 'none';
  spinner.style.display = 'inline-block';

  try {
    const body = new URLSearchParams({ title, section });
    if (descr) body.append('descr', descr);

    const res = await fetch(`${baseUrl()}/tasks`, {
      method: 'POST',
      credentials: 'include',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: body.toString(),
    });

    if (res.ok || res.status === 201) {
      showToast('Task added!', 'success');
      form.querySelector('#task-title').value = '';
      form.querySelector('#task-descr').value = '';
      form.querySelector('#task-title').focus();
    } else if (res.status === 401 || res.status === 303) {
      showAuthScreen();
    } else {
      showToast(`Error: ${res.status}`, 'error');
    }
  } catch (err) {
    showToast('Network error', 'error');
  } finally {
    submitBtn.disabled = false;
    label.style.display = '';
    spinner.style.display = 'none';
  }
}

// ─── Search ───────────────────────────────────────────────────────────────────

let searchTimer = null;

async function performSearch(q) {
  const container = document.getElementById('search-results');
  if (!q || q.length < 2) {
    container.innerHTML = '<div class="search-empty">Type at least 2 characters...</div>';
    return;
  }

  container.innerHTML = '<div class="search-empty"><span class="spinner"></span></div>';

  try {
    const html = await fetchJson(`/search?q=${encodeURIComponent(q)}`);
    renderSearchResults(html, container, q);
  } catch (err) {
    container.innerHTML = '<div class="search-empty">Search failed</div>';
  }
}

/**
 * Parse the search results HTML (the Search.Results JinjaX component).
 * We look for links or list items in the rendered HTML.
 */
function renderSearchResults(html, container, q) {
  const parser = new DOMParser();
  const doc = parser.parseFromString(html, 'text/html');

  // The server renders anchor tags for search results
  const links = doc.querySelectorAll('a[href]');

  if (links.length === 0) {
    container.innerHTML = `<div class="search-empty">No results for "<strong>${escHtml(q)}</strong>"</div>`;
    return;
  }

  let html2 = '';
  for (const link of links) {
    const href = link.getAttribute('href');
    if (!href || href.startsWith('#')) continue;
    const text = link.textContent.trim();
    if (!text) continue;

    // Determine type from path
    let type = 'task';
    let icon = taskIcon();
    if (href.includes('/list/')) { type = 'list'; icon = listIcon(); }
    else if (href.includes('/space/')) { type = 'space'; icon = spaceIcon(); }

    html2 += `
      <a class="search-result-item" href="${escHtml(href)}" data-path="${escHtml(href)}">
        <span class="search-result-icon">${icon}</span>
        <span class="search-result-text">${escHtml(text)}</span>
        <span class="search-result-type">${type}</span>
      </a>`;
  }

  if (!html2) {
    container.innerHTML = `<div class="search-empty">No results for "<strong>${escHtml(q)}</strong>"</div>`;
    return;
  }
  container.innerHTML = html2;

  // Wire up clicks to open in new tab
  container.querySelectorAll('.search-result-item').forEach((el) => {
    el.addEventListener('click', (e) => {
      e.preventDefault();
      const path = el.dataset.path || el.getAttribute('href');
      openInApp(path);
    });
  });
}

function taskIcon() {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor"><path fill-rule="evenodd" d="M16.704 4.153a.75.75 0 0 1 .143 1.052l-8 10.5a.75.75 0 0 1-1.127.075l-4.5-4.5a.75.75 0 0 1 1.06-1.06l3.894 3.893 7.48-9.817a.75.75 0 0 1 1.05-.143Z" clip-rule="evenodd"/></svg>`;
}

function listIcon() {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor"><path fill-rule="evenodd" d="M2 4.75A.75.75 0 0 1 2.75 4h14.5a.75.75 0 0 1 0 1.5H2.75A.75.75 0 0 1 2 4.75Zm0 10.5a.75.75 0 0 1 .75-.75h7.5a.75.75 0 0 1 0 1.5h-7.5a.75.75 0 0 1-.75-.75ZM2 10a.75.75 0 0 1 .75-.75h14.5a.75.75 0 0 1 0 1.5H2.75A.75.75 0 0 1 2 10Z" clip-rule="evenodd"/></svg>`;
}

function spaceIcon() {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor"><path d="M4.75 3A1.75 1.75 0 0 0 3 4.75v2.752l.104-.002h13.792c.035 0 .07 0 .104.002V6.75A1.75 1.75 0 0 0 15.25 5h-3.836a.25.25 0 0 1-.177-.073L9.823 3.513A1.75 1.75 0 0 0 8.586 3H4.75ZM3.104 9a1.75 1.75 0 0 0-1.673 2.265l1.385 4.5A1.75 1.75 0 0 0 4.488 17h11.023a1.75 1.75 0 0 0 1.673-1.235l1.384-4.5A1.75 1.75 0 0 0 16.895 9H3.104Z"/></svg>`;
}

// ─── Browse / Lists ───────────────────────────────────────────────────────────

async function loadListsNav() {
  const container = document.getElementById('lists-nav');
  try {
    const html = await fetchJson('/inbox');
    // Try to parse nav links from the sidebar
    const parser = new DOMParser();
    const doc = parser.parseFromString(html, 'text/html');

    // Look for list links in the navigation
    const listLinks = doc.querySelectorAll('a[href*="/list/"]');
    if (listLinks.length === 0) {
      container.innerHTML = '<div class="search-empty" style="padding:12px 0">No lists found</div>';
      return;
    }

    let html2 = '';
    const seen = new Set();
    for (const link of listLinks) {
      const href = link.getAttribute('href');
      if (seen.has(href)) continue;
      seen.add(href);
      const text = link.textContent.trim();
      if (!text) continue;

      // Extract color if available
      const colorEl = link.querySelector('[style*="color"]') || link.querySelector('[class*="bg-"]');
      const dotColor = '#6366f1';

      html2 += `
        <a class="nav-link" href="#" data-path="${escHtml(href)}">
          <span class="link-dot" style="background:${dotColor}"></span>
          <span>${escHtml(text)}</span>
        </a>`;
    }

    container.innerHTML = html2 || '<div class="search-empty" style="padding:12px 0">No lists found</div>';
    container.querySelectorAll('.nav-link').forEach((el) => {
      el.addEventListener('click', (e) => {
        e.preventDefault();
        openInApp(el.dataset.path);
      });
    });
  } catch {
    container.innerHTML = '<div class="search-empty" style="padding:12px 0">Failed to load lists</div>';
  }
}

// ─── Navigation ───────────────────────────────────────────────────────────────

function openInApp(path) {
  const url = `${baseUrl()}${path}`;
  chrome.tabs.create({ url });
  window.close();
}

// ─── UI state ────────────────────────────────────────────────────────────────

function showAuthScreen() {
  document.getElementById('auth-screen').style.display = '';
  document.getElementById('main-screen').style.display = 'none';
  setStatus('Not signed in', false);
}

function showMainScreen() {
  document.getElementById('auth-screen').style.display = 'none';
  document.getElementById('main-screen').style.display = '';
}

// ─── Tabs ────────────────────────────────────────────────────────────────────

function activateTab(tabId) {
  document.querySelectorAll('.tab-btn').forEach((b) =>
    b.classList.toggle('active', b.dataset.tab === tabId)
  );
  document.querySelectorAll('.tab-panel').forEach((p) =>
    p.classList.toggle('active', p.id === `tab-${tabId}`)
  );

  if (tabId === 'search') {
    document.getElementById('search-input').focus();
  }
  if (tabId === 'links') {
    loadListsNav();
  }
}

// ─── Pre-fill from context menu ───────────────────────────────────────────────

async function applyPrefill() {
  const isPrefill = new URLSearchParams(location.search).get('prefill') === '1';
  if (!isPrefill) return;

  return new Promise((resolve) => {
    chrome.storage.session.get('quickAddPrefill', ({ quickAddPrefill }) => {
      if (quickAddPrefill) {
        if (quickAddPrefill.title) {
          document.getElementById('task-title').value = quickAddPrefill.title;
        }
        if (quickAddPrefill.descr) {
          document.getElementById('task-descr').value = quickAddPrefill.descr;
        }
        chrome.storage.session.remove('quickAddPrefill');
      }
      resolve();
    });
  });
}

// ─── Utility ──────────────────────────────────────────────────────────────────

function escHtml(str) {
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

// ─── Init ────────────────────────────────────────────────────────────────────

async function init() {
  await loadSettings();
  applyTheme(settings.theme);

  // Check current page info to pre-fill from active tab if needed
  // (only when NOT in prefill mode — context menu handles its own pre-fill)
  const isPrefill = new URLSearchParams(location.search).get('prefill') === '1';

  setStatus('Checking connection...');

  const authResult = await checkAuth();

  if (authResult === null) {
    setStatus('Cannot reach ' + baseUrl(), false);
    showAuthScreen();
    return;
  }

  if (!authResult) {
    showAuthScreen();
    return;
  }

  showMainScreen();
  setStatus('Connected to ' + baseUrl(), true);

  // Load quick-add data in the background
  loadQuickAddData().then(() => {
    // Apply default list from settings if configured
    if (settings.defaultList) {
      const listSel = document.getElementById('list-select');
      const matching = Array.from(listSel.options).find(
        (o) => o.value === settings.defaultList || o.text.includes(settings.defaultList)
      );
      if (matching) {
        listSel.value = matching.value;
        onListChange(matching.value);
      }
    }
  });

  // Apply prefill (context menu quick-add)
  await applyPrefill();

  // ── Event listeners ──────────────────────────────────────────

  // Tab switching
  document.querySelectorAll('.tab-btn').forEach((btn) => {
    btn.addEventListener('click', () => activateTab(btn.dataset.tab));
  });

  // Space / list / section cascade
  document.getElementById('space-select').addEventListener('change', (e) => {
    onSpaceChange(e.target.value);
  });

  document.getElementById('list-select').addEventListener('change', (e) => {
    onListChange(e.target.value);
  });

  // Quick add form submit
  const form = document.getElementById('quick-add-form');
  form.addEventListener('submit', (e) => {
    e.preventDefault();
    submitTask(form);
  });

  // Ctrl/Cmd+Enter in textarea
  document.getElementById('task-descr').addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      submitTask(form);
    }
  });

  // Cancel button clears the form
  document.getElementById('btn-cancel').addEventListener('click', () => {
    form.querySelector('#task-title').value = '';
    form.querySelector('#task-descr').value = '';
    form.querySelector('#task-title').focus();
  });

  // Search
  document.getElementById('search-input').addEventListener('input', (e) => {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(() => performSearch(e.target.value.trim()), 300);
  });

  // Open side panel
  document.getElementById('btn-sidepanel').addEventListener('click', () => {
    chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
      if (tabs[0]) {
        chrome.sidePanel.open({ tabId: tabs[0].id });
        window.close();
      }
    });
  });

  // Open full app
  document.getElementById('btn-open-app').addEventListener('click', () => {
    openInApp('/inbox');
  });

  // Settings button
  document.getElementById('btn-settings').addEventListener('click', () => {
    chrome.runtime.openOptionsPage();
    window.close();
  });

  // Sign in button (auth screen)
  document.getElementById('btn-signin').addEventListener('click', () => {
    openInApp('/signin');
  });

  // Browse tab — nav links
  document.getElementById('link-inbox').addEventListener('click', (e) => {
    e.preventDefault();
    openInApp('/inbox');
  });

  document.getElementById('link-dashboard').addEventListener('click', (e) => {
    e.preventDefault();
    openInApp('/dashboard');
  });

  // Keyboard shortcuts inside popup
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      window.close();
    }
  });

  // Focus title input on load (add tab is default)
  document.getElementById('task-title').focus();
}

document.addEventListener('DOMContentLoaded', init);
