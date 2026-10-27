import { request as fetch } from './request.js';
import { toast } from './toast.js';
import { setSaveState } from './quiet-info.js';
if (
  location.pathname === '/settings/notifications' ||
  location.pathname === '/notifications'
) {
  const form = document.querySelector('main form[method=post]');
  form?.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (form.dataset.saving === 'true') return;
    form.dataset.saving = 'true';
    const button = event.submitter;
    const payload = new URLSearchParams(new FormData(form));
    if (button) button.disabled = true;
    setSaveState('notification-preferences', 'saving');
    try {
      const response = await fetch(form.action, { method: 'POST', body: payload });
      if (!response.ok) {
        const data = await response.json().catch(() => null);
        throw Error(data?.error?.message || 'Could not save notification preferences.');
      }
      if (new URL(response.url).pathname === '/signin')
        throw Error('Sign in again to save preferences.');
      setSaveState('notification-preferences', 'saved');
      toast('Notification preferences saved.');
    } catch (error) {
      setSaveState('notification-preferences', 'error', error.message);
    } finally {
      delete form.dataset.saving;
      if (button) button.disabled = false;
    }
  });
}
