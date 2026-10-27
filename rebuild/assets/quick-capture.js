import { request as fetch } from './request.js';
import { enqueueCapture, replayCaptures } from './offline.js';
import { toast } from './toast.js';
let dialog;
let loading = false;
export async function openCapture() {
  if (dialog?.open || loading) return;
  if (dialog) {
    dialog.showModal();
    dialog.querySelector('[name=title]')?.focus();
    return;
  }
  loading = true;
  try {
    const list =
      document.body.dataset.list ||
      document.querySelector('[data-task-page]')?.dataset.listId;
    const response = await fetch(
      '/capture' + (list ? `?list=${encodeURIComponent(list)}` : ''),
    );
    if (!response.ok) throw Error('Could not open capture. Try again when connected.');
    const doc = new DOMParser().parseFromString(await response.text(), 'text/html');
    const form = doc.querySelector('form[action="/capture"]');
    if (!form) throw Error('Create a space before adding a task.');
    dialog = document.createElement('dialog');
    dialog.className = 'capture-omnibar';
    dialog.setAttribute('aria-labelledby', 'quick-capture-heading');
    const heading = document.createElement('h2');
    heading.id = 'quick-capture-heading';
    heading.textContent = 'What’s on your mind?';
    const close = document.createElement('button');
    close.type = 'button';
    close.className = 'capture-close';
    close.innerHTML =
      '<svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#close"/></svg>';
    close.setAttribute('aria-label', 'Close capture');
    close.onclick = () => dialog.close();
    let outsidePress = false;
    const outside = (event) => {
      const r = dialog.getBoundingClientRect();
      return (
        event.target === dialog &&
        (event.clientX < r.left ||
          event.clientX > r.right ||
          event.clientY < r.top ||
          event.clientY > r.bottom)
      );
    };
    dialog.addEventListener('pointerdown', (event) => {
      outsidePress = outside(event);
    });
    dialog.addEventListener('click', (event) => {
      if (outsidePress && outside(event)) dialog.close();
      outsidePress = false;
    });
    form.className = 'quick-capture-form';
    const details = form.querySelector('details');
    if (details) {
      details.querySelector('summary')?.remove();
      details.replaceWith(...details.childNodes);
    }
    form.querySelector('p')?.remove();
    form.querySelector('[name=description]').rows = 2;
    const error = document.createElement('p');
    error.setAttribute('role', 'alert');
    dialog.append(close, heading, form, error);
    document.body.append(dialog);
    form.addEventListener('submit', async (event) => {
      event.preventDefault();
      const button = form.querySelector('[type=submit]');
      if (button.disabled) return;
      button.disabled = true;
      error.textContent = '';
      try {
        const data = new FormData(form);
        const account = document.querySelector('meta[name=account-id]')?.content;
        if (!account) throw Error('Sign in again to capture this task.');
        await enqueueCapture(
          account,
          data.get('list_id'),
          data.get('title').trim(),
          data.get('description'),
        );
        form.elements.title.value = '';
        form.elements.description.value = '';
        dialog.close();
        const result = await replayCaptures(account, async (record) => {
          const r = await fetch('/api/v1/capture', {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json',
              'X-CSRF-Token': data.get('csrf'),
              'Idempotency-Key': record.key,
            },
            body: JSON.stringify(record.payload),
          });
          const body = await r.json();
          if (!r.ok)
            throw Error(body.error?.message || 'Capture will retry when connected.');
          return body;
        });
        if (result.errors.length)
          toast('Task kept on this device. Open Inbox to retry syncing.');
        else if (location.pathname === '/today' || location.pathname === '/upcoming')
          location.reload();
      } catch (e) {
        error.textContent = e.message;
        if (!dialog.open) dialog.showModal();
      } finally {
        button.disabled = false;
      }
    });
    dialog.showModal();
    form.elements.title.focus();
  } catch (error) {
    toast(error.message);
  } finally {
    loading = false;
  }
}
document.addEventListener('click', (event) => {
  const link = event.target.closest('[data-quick-capture],a[href^="/capture"]');
  if (!link) return;
  event.preventDefault();
  openCapture();
});
document.addEventListener(
  'keydown',
  (event) => {
    if (
      event.key.toLowerCase() !== 'n' ||
      event.ctrlKey ||
      event.metaKey ||
      event.altKey ||
      event.target.closest('input,textarea,select,[contenteditable=true]') ||
      document.querySelector('dialog[open]')
    )
      return;
    event.preventDefault();
    event.stopImmediatePropagation();
    openCapture();
  },
  true,
);
if (document.body.classList.contains('planning-list-page')) {
  const button = document.querySelector('.sidebar-add');
  if (button) {
    button.removeAttribute('data-list-create-open');
    button.dataset.quickCapture = '';
    button.setAttribute('aria-label', 'Add task');
    if (button.tagName === 'A') button.href = '/capture';
    button.querySelector('span').textContent = 'Add task';
  }
}
