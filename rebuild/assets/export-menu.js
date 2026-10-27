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
  const cancel = document.createElement('button');
  cancel.textContent = 'Cancel';
  cancel.onclick = () => d.close();
  d.append(title, hint, formats, cancel);
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
