import { request } from './request.js';
import { toast } from './toast.js';
const form = document.querySelector('[data-capture-preferences]');
if (form) {
  const status = form.querySelector('[data-capture-preferences-status]');
  const retry = form.querySelector('[data-capture-retry]');
  const fields = [...form.querySelectorAll('input, textarea, button[type=submit]')];
  const dates = form.querySelector('[data-date-options]');
  const sync = () => {
    dates.hidden = !form.elements.smart_enabled.checked;
  };
  const busy = (value) => {
    form.setAttribute('aria-busy', String(value));
    fields.forEach((field) => {
      field.disabled = value;
    });
  };
  async function load() {
    busy(true);
    retry.hidden = true;
    status.textContent = 'Loading preferences…';
    try {
      const response = await request('/api/v1/capture/preferences', {
        cache: 'no-store',
      });
      if (!response.ok) throw Error('Could not load preferences. Please try again.');
      const value = await response.json();
      for (const key of ['smart_enabled', 'date_enabled', 'repeat_capture'])
        form.elements[key].checked = value[key];
      form.elements.ai_instructions.value = value.ai_instructions;
      form.querySelector('[data-ai-instructions]').open = Boolean(
        value.ai_instructions.trim(),
      );
      sync();
      status.textContent = '';
      busy(false);
    } catch (error) {
      status.textContent = error.message;
      form.setAttribute('aria-busy', 'false');
      retry.hidden = false;
    }
  }
  retry.addEventListener('click', load);
  form.elements.smart_enabled.addEventListener('change', sync);
  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (form.getAttribute('aria-busy') === 'true') return;
    const data = { ai_instructions: form.elements.ai_instructions.value };
    for (const key of ['smart_enabled', 'date_enabled', 'repeat_capture'])
      data[key] = form.elements[key].checked;
    busy(true);
    status.textContent = 'Saving…';
    try {
      const response = await request('/api/v1/capture/preferences', {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
          'X-CSRF-Token': document.querySelector('meta[name=csrf-token]').content,
        },
        body: JSON.stringify(data),
      });
      if (!response.ok)
        throw Error(
          (await response.json()).error?.message ||
            'Could not save preferences. Your changes are still here.',
        );
      status.textContent = '';
      toast('Preferences saved.');
    } catch (error) {
      status.textContent = error.message;
    } finally {
      busy(false);
    }
  });
  load();
}
for (const all of document.querySelectorAll('[data-credential-all]')) {
  const lists = all.closest('form').querySelector('[data-credential-lists]');
  const sync = () => {
    lists.hidden = all.checked;
    lists.querySelectorAll('input').forEach((input) => {
      input.disabled = all.checked;
    });
  };
  all.addEventListener('change', sync);
  sync();
}
