import Alpine from './alpine-csp.min.js';
import './alpine.js';

export function openSearch() {
  const dialog = document.querySelector('[data-search-dialog]');
  Alpine.$data(dialog).open();
}
