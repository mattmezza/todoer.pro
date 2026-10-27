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
  button.disabled = true;
  preferences()
    .then((value) => {
      form.elements.timezone.value = value.timezone;
      form.elements.week_start.value = String(value.week_start ?? 1);
      button.disabled = false;
    })
    .catch((error) => {
      status.textContent = error.message;
    });
  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    button.disabled = true;
    status.textContent = 'Saving…';
    try {
      const value = await preferences();
      value.timezone = form.elements.timezone.value.trim();
      value.week_start = Number(form.elements.week_start.value);
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
      status.textContent = 'Saved.';
    } catch (error) {
      status.textContent = error.message;
    } finally {
      button.disabled = false;
    }
  });
}
