import Alpine from './alpine-csp.min.js';

// Keep the report API used by the planning workbench; the component owns its UI.
export function openExport(scope) {
  const dialog = document.querySelector('[data-export-dialog]');
  Alpine.$data(dialog).present(scope);
}
