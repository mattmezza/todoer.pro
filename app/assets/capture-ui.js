import { enqueueCapture } from './offline.js';
import { mountSmartCapture } from './smart-capture.js';
export function mountCaptures({
  account,
  listId,
  canWrite,
  notice,
  onQueued,
  setBusy,
  onSettled,
}) {
  const key = (section) => `todoer:${account}:capture:${listId}:${section || 'main'}`;
  const section = (form) => form.dataset.captureSection || null;
  const title = (form) => form.querySelector('[name="title"]');
  const description = (form) => form.querySelector('[name="description"]');
  function persist(form) {
    sessionStorage.setItem(
      key(section(form)),
      JSON.stringify({ title: title(form).value, description: description(form).value }),
    );
  }
  function restore() {
    for (const form of document.querySelectorAll('.inline-capture')) {
      if (!canWrite) {
        form.hidden = true;
        continue;
      }
      let saved;
      try {
        saved = JSON.parse(sessionStorage.getItem(key(section(form))) || 'null');
      } catch {
        saved = null;
      }
      if (saved) {
        title(form).value = saved.title || '';
        description(form).value = saved.description || '';
      }
      mountSmartCapture(form);
      if (!form.dataset.captureMounted) {
        form.hidden = !(title(form).value || description(form).value);
        form.dataset.captureMounted = 'true';
      }
    }
  }
  restore();
  document.body.addEventListener('htmx:afterSwap', restore);
  document.addEventListener('input', (event) => {
    const form = event.target.closest('.inline-capture');
    if (form) persist(form);
  });
  document.addEventListener('click', (event) => {
    const trigger = event.target.closest('#capture-open,[data-section-capture]');
    if (trigger) {
      const form =
        trigger.id === 'capture-open'
          ? document.querySelector('#capture-form')
          : trigger.closest('.task-section').querySelector('.section-capture');
      if (form && canWrite) {
        form.hidden = false;
        title(form).focus();
      }
    }
    const cancel = event.target.closest('#capture-cancel,[data-capture-cancel]');
    if (cancel) {
      const form = cancel.closest('.inline-capture');
      persist(form);
      form.hidden = true;
      (form.classList.contains('section-capture')
        ? form.closest('.task-section').querySelector('[data-section-capture]')
        : document.querySelector('#capture-open')
      )?.focus();
    }
  });
  document.addEventListener('submit', async (event) => {
    const form = event.target.closest('.inline-capture');
    if (!form) return;
    event.preventDefault();
    if (!canWrite || form.dataset.busy === 'true') return;
    await mountSmartCapture(form).ready;
    const interpreted = mountSmartCapture(form).get();
    const text = interpreted.title,
      brief = description(form).value;
    if (!text) return;
    form.dataset.busy = 'true';
    setBusy(true);
    const button = form.querySelector('[type="submit"]');
    button.disabled = true;
    try {
      await enqueueCapture(
        account,
        interpreted.list_id || listId,
        text,
        brief,
        section(form) || interpreted.section_id || null,
        interpreted.metadata,
      );
      title(form).value = '';
      mountSmartCapture(form).reset();
      description(form).value = '';
      sessionStorage.removeItem(key(section(form)));
      if (!section(form)) sessionStorage.removeItem(`todoer:${account}:draft:${listId}`);
      notice('Capture saved on this device.');
      await onQueued();
      if (form.isConnected) title(form).focus();
    } catch (error) {
      notice(`Could not save capture: ${error.message}. Your draft is preserved.`, true);
      persist(form);
    } finally {
      form.dataset.busy = 'false';
      button.disabled = false;
      setBusy(false);
      onSettled();
    }
  });
}
