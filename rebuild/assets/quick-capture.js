import { enqueueCapture, replayCaptures } from './offline.js';
import { toast } from './toast.js';
let dialog;
let loading = false;
export async function openCapture() {
  if (dialog?.open || loading) return;
  if (dialog) { dialog.showModal(); dialog.querySelector('[name=title]')?.focus(); return; }
  loading = true;
  try {
    const list = document.body.dataset.list || document.querySelector('[data-task-page]')?.dataset.listId;
    const response = await fetch('/capture' + (list ? `?list=${encodeURIComponent(list)}` : ''));
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
    close.type = 'button'; close.className = 'capture-close';
    close.textContent = '×'; close.setAttribute('aria-label', 'Close capture');
    close.onclick = () => dialog.close();
    form.className = 'quick-capture-form';
    const details = form.querySelector('details');
    if (details) { details.querySelector('summary')?.remove(); details.replaceWith(...details.childNodes); }
    form.querySelector('p')?.remove();
    form.querySelector('[name=description]').rows = 2;
    const error = document.createElement('p'); error.setAttribute('role', 'alert');
    dialog.append(close, heading, form, error); document.body.append(dialog);
    form.addEventListener('submit', async (event) => {
      event.preventDefault();
      const button = form.querySelector('[type=submit]');
      if (button.disabled) return;
      button.disabled = true; error.textContent = '';
      try {
        const data = new FormData(form);
        const account = document.querySelector('meta[name=account-id]')?.content;
        if (!account) throw Error('Sign in again to capture this task.');
        await enqueueCapture(account, data.get('list_id'), data.get('title').trim(), data.get('description'));
        form.elements.title.value = ''; form.elements.description.value = '';
        dialog.close();
        const result = await replayCaptures(account, async (record) => {
          const r = await fetch('/api/v1/capture', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': data.get('csrf'), 'Idempotency-Key': record.key },
            body: JSON.stringify(record.payload),
          });
          const body = await r.json();
          if (!r.ok) throw Error(body.error?.message || 'Capture will retry when connected.');
          return body;
        });
        if (result.errors.length) toast('Task kept on this device. Open Inbox to retry syncing.');
        else if (location.pathname === '/today' || location.pathname === '/upcoming') location.reload();
      } catch (e) { error.textContent = e.message; if (!dialog.open) dialog.showModal(); }
      finally { button.disabled = false; }
    });
    dialog.showModal(); form.elements.title.focus();
  } catch (error) { toast(error.message); }
  finally { loading = false; }
}
document.addEventListener('click', (event) => {
  const link = event.target.closest('[data-quick-capture],a[href^="/capture"]');
  if (!link) return;
  event.preventDefault(); openCapture();
});
document.addEventListener('keydown', (event) => {
  if (event.key.toLowerCase() !== 'n' || event.ctrlKey || event.metaKey || event.altKey ||
      event.target.closest('input,textarea,select,[contenteditable=true]') || document.querySelector('dialog[open]')) return;
  event.preventDefault(); event.stopImmediatePropagation(); openCapture();
}, true);
if (document.body.classList.contains('planning-list-page')) {
  const button = document.querySelector('.sidebar-add');
  if (button) {
    button.removeAttribute('data-list-create-open'); button.dataset.quickCapture = '';
    button.setAttribute('aria-label', 'Add task');
    if (button.tagName === 'A') button.href = '/capture';
    button.querySelector('span').textContent = 'Add task';
  }
  const toy = document.createElement('button');
  toy.className = 'task-play'; toy.type = 'button';
  toy.setAttribute('aria-label', 'Playfully check off a little task');
  toy.innerHTML = '<span class="play-card play-card-back" aria-hidden="true"></span><span class="play-card" aria-hidden="true"><span class="play-check">✓</span><span class="play-line"></span><span class="play-line short"></span></span>';
  let launching = false;
  toy.addEventListener('click', async () => {
    if (launching) return;
    launching = true;
    toy.classList.add('is-checked');
    const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
    await new Promise((resolve) => setTimeout(resolve, reduced ? 220 : 110));
    if (!reduced) {
      const card = toy.querySelector('.play-card:not(.play-card-back)');
      const rect = card.getBoundingClientRect();
      const flyer = card.cloneNode(true);
      flyer.classList.add('task-flyer');
      flyer.setAttribute('aria-hidden', 'true');
      Object.assign(flyer.style, {
        left: `${rect.left}px`, top: `${rect.top}px`,
        width: `${rect.width}px`, height: `${rect.height}px`,
      });
      document.body.append(flyer);
      // Fire through an upward semicircle, with varied speed and spin.
      const angle = (-165 + Math.random() * 150) * Math.PI / 180;
      const speed = 650 + Math.random() * 850;
      const duration = 650 + Math.random() * 430;
      const distance = speed * duration / 1000;
      const x = Math.cos(angle) * distance;
      const y = Math.sin(angle) * distance;
      const spin = (Math.random() > .5 ? 1 : -1) * (160 + Math.random() * 440);
      flyer.animate([
        { transform: 'translate(0, 0) rotate(-8deg) scale(1)', opacity: 1 },
        { transform: `translate(${x * .35}px, ${y * .48}px) rotate(${spin * .35}deg) scale(1.08)`, opacity: 1, offset: .4 },
        { transform: `translate(${x}px, ${y}px) rotate(${spin}deg) scale(.55)`, opacity: 0 },
      ], { duration, easing: 'cubic-bezier(.12,.55,.32,1)', fill: 'forwards' })
        .finished.finally(() => flyer.remove());
      card.animate([
        { opacity: 0, transform: 'translateY(12px) scale(.85)' },
        { opacity: 1, transform: 'translateY(0) scale(1)' },
      ], { duration: 240, delay: 60, fill: 'backwards', easing: 'cubic-bezier(.2,.8,.2,1)' });
    }
    toy.classList.remove('is-checked');
    launching = false;
  });
  document.body.append(toy);
}
