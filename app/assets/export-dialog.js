import { createScope } from './page-lifecycle.js';
import { setSaveState } from './quiet-info.js';
import { outsideDialog } from './dialog.js';

export function exportDialog() {
  const scope = createScope();
  let dialog;
  let report;
  let outsidePress = false;
  let generation = 0;
  return {
    formats: ['pdf', 'txt', 'csv', 'tsv', 'json'],
    busy: false,
    init() {
      dialog = this.$el;
    },
    destroy() {
      scope.dispose();
    },
    present(value) {
      report = value;
      generation++;
      this.busy = false;
      dialog.showModal();
    },
    openFromTrigger(event) {
      const button = event.target.closest('[data-export-kind]');
      if (!button) return;
      event.preventDefault();
      button.closest('details')?.removeAttribute('open');
      this.present({ kind: button.dataset.exportKind, id: button.dataset.exportId });
    },
    close() {
      dialog.close();
    },
    closed() {
      generation++;
    },
    backdropPress(event) {
      outsidePress = outsideDialog(dialog, event);
    },
    backdropClick(event) {
      if (outsidePress && outsideDialog(dialog, event)) this.close();
      outsidePress = false;
    },
    async download(event) {
      if (this.busy) return;
      this.busy = true;
      const format = event.currentTarget.dataset.format;
      const current = generation;
      setSaveState('export', 'saving', 'Preparing your export…');
      try {
        const response = await scope.request(
          '/api/v1/reports/export?' + new URLSearchParams({ ...report, format }),
          { cache: 'no-store' },
        );
        if (!response.ok) {
          const error = await response.json();
          throw Error(error.error?.message || 'Could not export.');
        }
        const blob = await response.blob();
        if (current !== generation || scope.signal.aborted) return;
        const url = URL.createObjectURL(blob);
        const link = document.createElement('a');
        link.href = url;
        link.download = `todoer-${report.kind}.${format}`;
        link.click();
        // Revoke even if the page component is destroyed in the meantime.
        setTimeout(() => URL.revokeObjectURL(url), 10000);
        setSaveState('export', 'saved', 'Export downloaded.');
        this.close();
      } catch (error) {
        if (current === generation && !scope.signal.aborted)
          setSaveState('export', 'error', error.message);
      } finally {
        if (current === generation) this.busy = false;
      }
    },
  };
}
