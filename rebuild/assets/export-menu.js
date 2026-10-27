import { setSaveState } from './quiet-info.js';
export function openExport(scope) {
  document.querySelector('[data-export-dialog]')?.close();
  const d = document.createElement('dialog');
  d.className = 'quick-dialog export-dialog';
  d.dataset.exportDialog = '';
  const title = document.createElement('h2');
  title.textContent = 'Export';
  const hint = document.createElement('p');
  hint.textContent = 'Choose a format to download.';
  const formats = document.createElement('div');
  formats.className = 'export-formats';
  for (const format of ['pdf', 'txt', 'csv', 'tsv', 'json']) {
    const b = document.createElement('button');
    b.type = 'button';
    b.textContent = format.toUpperCase();
    b.onclick = async () => {
      b.disabled = true;
      setSaveState('export', 'saving', 'Preparing your export…');
      try {
        const r = await fetch(
          '/api/v1/reports/export?' + new URLSearchParams({ ...scope, format }),
          { cache: 'no-store' },
        );
        if (!r.ok) {
          const err = await r.json();
          throw Error(err.error?.message || 'Could not export.');
        }
        const url = URL.createObjectURL(await r.blob());
        const a = document.createElement('a');
        a.href = url;
        a.download = `todoer-${scope.kind}.${format}`;
        a.click();
        setTimeout(() => URL.revokeObjectURL(url), 10000);
        setSaveState('export', 'saved', 'Export downloaded.');
        d.close();
      } catch (e) {
        setSaveState('export', 'error', e.message);
      } finally {
        b.disabled = false;
      }
    };
    formats.append(b);
  }
  const close = document.createElement('button');
  close.type = 'button';
  close.className = 'dialog-close';
  close.setAttribute('aria-label', 'Close export');
  close.innerHTML =
    '<svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#close"/></svg>';
  close.onclick = () => d.close();
  d.append(title, hint, formats, close);
  let backdropPress = false;
  const outside = (event) => {
    const r = d.getBoundingClientRect();
    return (
      event.target === d &&
      (event.clientX < r.left ||
        event.clientX > r.right ||
        event.clientY < r.top ||
        event.clientY > r.bottom)
    );
  };
  d.addEventListener('pointerdown', (event) => (backdropPress = outside(event)));
  d.addEventListener('click', (event) => {
    if (backdropPress && outside(event)) d.close();
    backdropPress = false;
  });
  d.addEventListener('close', () => d.remove(), { once: true });
  document.body.append(d);
  d.showModal();
}
document.addEventListener('click', (e) => {
  const b = e.target.closest('[data-export-kind]');
  if (!b) return;
  e.preventDefault();
  b.closest('details')?.removeAttribute('open');
  openExport({ kind: b.dataset.exportKind, id: b.dataset.exportId });
});
