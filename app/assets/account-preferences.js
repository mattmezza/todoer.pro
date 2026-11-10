// Fresh on every server-rendered navigation; no storage or cross-account cache.
export function pagePreferences() {
  try {
    return JSON.parse(
      document.querySelector('#navigation')?.dataset.preferences || 'null',
    );
  } catch {
    return null;
  }
}
export function updatePagePreferences(value) {
  const navigation = document.querySelector('#navigation');
  if (navigation) navigation.dataset.preferences = JSON.stringify(value);
}
