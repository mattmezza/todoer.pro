import { request as fetch } from './request.js';
import { toast } from './toast.js';
import { setSaveState } from './quiet-info.js';
let preferenceWrite = Promise.resolve();
function serializePreferenceWrite(action) {
  const next = preferenceWrite.then(action);
  preferenceWrite = next.catch(() => {});
  return next;
}
const form = document.querySelector('[data-calendar-preferences]');
if (form) {
  const status = form.querySelector('[data-preference-status]');
  const button = form.querySelector('button');
  const zones = document.querySelector('#account-timezones');
  for (const zone of ['UTC', ...(Intl.supportedValuesOf?.('timeZone') || [])]) {
    const option = document.createElement('option');
    option.value = zone;
    zones.append(option);
  }
  async function preferences() {
    const response = await fetch('/api/v1/notification-preferences', {
      cache: 'no-store',
    });
    if (!response.ok) throw Error('Could not load preferences.');
    return response.json();
  }
  const fields = [form.elements.timezone, form.elements.week_start];
  fields.forEach((field) => (field.disabled = true));
  button.disabled = true;
  preferences()
    .then((value) => {
      form.elements.timezone.value = value.timezone;
      form.elements.week_start.value = String(value.week_start ?? 1);
      button.disabled = false;
      fields.forEach((field) => (field.disabled = false));
    })
    .catch((error) => {
      status.textContent = '';
      setSaveState('preferences', 'error', error.message);
    });
  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (button.disabled) return;
    const choice = {
      timezone: form.elements.timezone.value.trim(),
      week_start: Number(form.elements.week_start.value),
    };
    fields.forEach((field) => (field.disabled = true));
    button.disabled = true;
    status.textContent = '';
    setSaveState('preferences', 'saving');
    try {
      await serializePreferenceWrite(async () => {
        const value = { ...(await preferences()), ...choice };
        const response = await fetch('/api/v1/notification-preferences', {
          method: 'PUT',
          headers: {
            'Content-Type': 'application/json',
            'X-CSRF-Token': document.querySelector('meta[name=csrf-token]').content,
          },
          body: JSON.stringify(value),
        });
        if (!response.ok)
          throw Error(
            (await response.json()).error?.message || 'Could not save preferences.',
          );
      });
      status.textContent = '';
      setSaveState('preferences', 'saved');
      toast('Preferences saved.');
    } catch (error) {
      status.textContent = '';
      setSaveState('preferences', 'error', error.message);
    } finally {
      fields.forEach((field) => (field.disabled = false));
      button.disabled = false;
    }
  });
}

const playForm = document.querySelector('[data-play-preferences]');
if (playForm) {
  const enabled = playForm.elements.fun_animations,
    style = playForm.elements.fun_animation;
  const status = playForm.querySelector('[data-play-status]');
  const controls = [enabled, style];
  const options = playForm.querySelector('[data-play-options]');
  controls.forEach((control) => (control.disabled = true));
  async function load() {
    const response = await fetch('/api/v1/notification-preferences', {
      cache: 'no-store',
    });
    if (!response.ok) throw Error('Could not load preferences.');
    return response.json();
  }
  load()
    .then((value) => {
      enabled.checked = value.fun_animations !== false;
      style.value = value.fun_animation || 'tasks';
      controls.forEach((control) => (control.disabled = false));
      style.disabled = !enabled.checked;
      options.hidden = !enabled.checked;
    })
    .catch((error) => setSaveState('preferences', 'error', error.message));
  playForm.addEventListener('change', async () => {
    const choice = { fun_animations: enabled.checked, fun_animation: style.value };
    options.hidden = !enabled.checked;
    controls.forEach((control) => (control.disabled = true));
    status.textContent = '';
    setSaveState('preferences', 'saving');
    try {
      const value = await serializePreferenceWrite(async () => {
        const value = { ...(await load()), ...choice };
        const response = await fetch('/api/v1/notification-preferences', {
          method: 'PUT',
          headers: {
            'Content-Type': 'application/json',
            'X-CSRF-Token': document.querySelector('meta[name=csrf-token]').content,
          },
          body: JSON.stringify(value),
        });
        if (!response.ok) throw Error('Could not save preferences. Try again.');
        return value;
      });
      document.dispatchEvent(
        new CustomEvent('todoer-play-preferences', { detail: value }),
      );
      status.textContent = '';
      setSaveState('preferences', 'saved');
      toast('Preferences saved.');
    } catch (error) {
      status.textContent = '';
      setSaveState('preferences', 'error', error.message);
    } finally {
      enabled.disabled = false;
      style.disabled = !enabled.checked;
      options.hidden = !enabled.checked;
    }
  });
}
