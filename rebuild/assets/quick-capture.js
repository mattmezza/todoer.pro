import { loadingDialog } from './loading.js';
import { request as fetch } from './request.js';
import { enqueueCapture, replayCaptures } from './offline.js';
import { toast } from './toast.js';
import { mountSmartCapture } from './smart-capture.js';
let dialog;
let loading = false;
export async function openCapture(listId) {
  if (dialog?.open || loading) return;
  if (dialog) {
    if (listId) {
      const destination = dialog.querySelector('[name=list_id]');
      destination.value = listId;
      destination.dispatchEvent(new Event('change'));
    }
    dialog.showModal();
    dialog.querySelector('[name=description]')?.dispatchEvent(new Event('input'));
    dialog.querySelector('[name=title]')?.focus();
    return;
  }
  loading = true;
  const pending = loadingDialog('Loading quick capture', 'capture-omnibar');
  try {
    const list =
      listId || document.body.dataset.list ||
      document.querySelector('[data-task-page]')?.dataset.listId;
    const response = await fetch(
      '/capture' + (list ? `?list=${encodeURIComponent(list)}` : ''),
    );
    if (!pending.dialog.open) return;
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
    const destination = form.querySelector('[name=list_id]');
    const destinationLabel = form.querySelector('label[for="capture-destination"]');
    destination.hidden = true;
    if (destinationLabel) destinationLabel.hidden = true;
    const details = form.querySelector('details');
    const description = form.querySelector('[name=description]');
    description.rows = 1;
    description.placeholder = 'Add description';
    description.classList.add('quick-capture-description');
    const descriptionLabel = details.querySelector('label');
    details.replaceWith(descriptionLabel, description);
    const resize = () => {
      description.style.height = '0px';
      description.style.height = `${description.scrollHeight + description.offsetHeight - description.clientHeight}px`;
    };
    description.addEventListener('input', resize);
    form.querySelector('p')?.remove();
    const smart = mountSmartCapture(form);
    const fields = document.createElement('div');
    fields.className = 'quick-capture-fields';
    const titleInput = form.querySelector('.smart-capture-input');
    titleInput.before(fields);
    fields.append(titleInput, descriptionLabel, description);
    const footer = document.createElement('div');
    footer.className = 'quick-capture-footer';
    const submit = form.querySelector('[type=submit]');
    submit.before(footer);
    const actions = document.createElement('div');
    actions.className = 'quick-capture-actions';
    const repeatLabel = document.createElement('label');
    repeatLabel.className = 'quick-capture-repeat';
    const repeat = document.createElement('input');
    repeat.type = 'checkbox';
    repeat.setAttribute('aria-label', 'Keep adding');
    repeatLabel.append(repeat, document.createTextNode('Keep adding'));
    const repeatKey = 'todoer.quickCapture.keepAdding';
    let savedRepeat = null;
    try {
      const stored = localStorage.getItem(repeatKey);
      if (stored === 'true' || stored === 'false') savedRepeat = stored === 'true';
    } catch (_) { /* Storage can be unavailable in private browsing. */ }
    let shiftHeld = false;
    const updateRepeat = () => { repeat.checked = Boolean(savedRepeat) !== shiftHeld; };
    updateRepeat();
    repeat.addEventListener('change', () => {
      savedRepeat = repeat.checked !== shiftHeld;
      try { localStorage.setItem(repeatKey, String(savedRepeat)); } catch (_) {}
    });
    smart.ready.then(() => {
      if (savedRepeat === null) {
        savedRepeat = smart.get().repeat;
        updateRepeat();
      }
    });
    document.addEventListener('keydown', (event) => {
      if (dialog.open && event.key === 'Shift' && !shiftHeld) {
        shiftHeld = true;
        updateRepeat();
      }
    });
    document.addEventListener('keyup', (event) => {
      if (event.key === 'Shift' && shiftHeld) {
        shiftHeld = false;
        updateRepeat();
      }
    });
    const releaseShift = () => {
      if (shiftHeld) {
        shiftHeld = false;
        updateRepeat();
      }
    };
    window.addEventListener('blur', releaseShift);
    dialog.addEventListener('close', releaseShift);
    actions.append(repeatLabel, submit);
    footer.append(smart.help, actions);
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
        const interpreted = { ...smart.get(), repeat: repeat.checked };
        const account = document.querySelector('meta[name=account-id]')?.content;
        if (!account) throw Error('Sign in again to capture this task.');
        await enqueueCapture(
          account,
          interpreted.list_id || data.get('list_id'),
          interpreted.title,
          data.get('description'),
          null,
          interpreted.metadata,
        );
        if (interpreted.repeat && interpreted.list_id) {
          form.elements.list_id.value = interpreted.list_id;
        }
        form.elements.title.value = '';
        form.elements.description.value = '';
        description.style.height = '';
        resize();
        smart.reset();
        if (!interpreted.repeat) dialog.close();
        else form.elements.title.focus();
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
          if (!r.ok) {
            const failure = Error(
              body.error?.message || 'Capture will retry when connected.',
            );
            failure.status = r.status;
            throw failure;
          }
          return body;
        });
        if (result.errors.length)
          toast('Task kept on this device. Open Inbox to retry syncing.');
        if (result.sent) document.dispatchEvent(new Event('todoer:workbench-refresh'));
        if (!result.errors.length &&
          !interpreted.repeat &&
          (location.pathname === '/today' || location.pathname === '/upcoming')
        )
          location.reload();
      } catch (e) {
        error.textContent = e.message;
        if (!dialog.open) dialog.showModal();
      } finally {
        button.disabled = false;
      }
    });
    pending.finish();
    dialog.showModal();
    resize();
    form.elements.title.focus();
  } catch (error) {
    toast(error.message);
  } finally {
    pending.finish();
    loading = false;
  }
}
document.addEventListener('click', (event) => {
  const link = event.target.closest('[data-quick-capture],a[href^="/capture"]');
  if (!link) return;
  event.preventDefault();
  openCapture(link.dataset.captureList);
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
