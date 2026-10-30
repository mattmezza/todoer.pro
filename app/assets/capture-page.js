import { mountSmartCapture } from './smart-capture.js';
import { enqueueCapture, replayCaptures } from './offline.js';
import { request } from './request.js';
const form = document.querySelector('form[action="/capture"]');
if (form) {
  const smart = mountSmartCapture(form),
    account = document.querySelector('meta[name=account-id]')?.content;
  const status = document.createElement('p');
  status.setAttribute('role', 'status');
  form.append(status);
  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    const submit = form.querySelector('[type=submit]');
    if (submit.disabled) return;
    submit.disabled = true;
    try {
      await smart.ready;
      const data = new FormData(form),
        value = smart.get(),
        list = value.list_id || data.get('list_id');
      if (!value.title) throw Error('Enter a task title.');
      await enqueueCapture(
        account,
        list,
        value.title,
        data.get('description'),
        null,
        value.metadata,
      );
      form.elements.title.value = '';
      form.elements.description.value = '';
      smart.reset();
      const result = await replayCaptures(account, async (record) => {
        const response = await request('/api/v1/capture', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            'X-CSRF-Token': data.get('csrf'),
            'Idempotency-Key': record.key,
          },
          body: JSON.stringify(record.payload),
        });
        const body = await response.json();
        if (!response.ok) {
          const error = Error(body.error?.message || 'Unable to sync');
          error.status = response.status;
          throw error;
        }
        return body;
      });
      if (result.errors.length)
        status.textContent = 'Saved on this device. Open Inbox to retry syncing.';
      else if (value.repeat) {
        form.elements.title.value = '';
        form.elements.description.value = '';
        smart.reset();
        status.textContent = 'Task added.';
        form.elements.title.focus();
      } else location.assign(`/lists/${encodeURIComponent(list)}`);
    } catch (error) {
      status.textContent = error.message;
    } finally {
      submit.disabled = false;
    }
  });
}
