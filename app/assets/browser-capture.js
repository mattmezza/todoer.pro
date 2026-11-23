const marker = '#todoer-capture&';
const key = 'todoer-browser-capture';

function readFragment() {
  if (!location.hash.startsWith(marker)) return null;
  const fields = new URLSearchParams(location.hash.slice(marker.length));
  history.replaceState(history.state, '', location.pathname + location.search);
  const title = (fields.get('title') ?? '').trim().slice(0, 500);
  if (!title) return null;

  let source = '';
  try {
    const url = new URL(fields.get('url') ?? '');
    if (['http:', 'https:'].includes(url.protocol) && !url.username && !url.password) {
      source = url.href.slice(0, 8000);
    }
  } catch {
    // A capture can still be edited when the source URL is missing.
  }
  const selection = (fields.get('selection') ?? '').trim().slice(0, 5000);
  return { title, description: [source, selection].filter(Boolean).join('\n\n') };
}

if (location.pathname === '/signin') {
  // The redirect from /capture inherits the fragment. Keep it in this window
  // while the user signs in and clear it from the address before continuing.
  const draft = readFragment();
  const next = new URLSearchParams(location.search).get('next');
  if (draft && next) {
    try {
      sessionStorage.setItem(key, JSON.stringify({ draft, next, created: Date.now() }));
    } catch {
      // The browser can disable session storage.
    }
  }
}

export function takeBrowserCapture() {
  const draft = readFragment();
  try {
    const saved = sessionStorage.getItem(key);
    sessionStorage.removeItem(key);
    if (draft || !saved) return draft;
    const pending = JSON.parse(saved);
    const age = Date.now() - pending.created;
    if (
      pending.next === location.pathname + location.search &&
      age >= 0 &&
      age < 900_000
    ) {
      return pending.draft;
    }
  } catch {
    // Session storage can be disabled or cleared.
  }
  return draft;
}
