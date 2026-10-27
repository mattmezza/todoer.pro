import { toast } from './toast.js';
import { openListSettings } from './list-settings.js';
document.addEventListener('click', async (event) => {
  const copy = event.target.closest('[data-copy-space]');
  if (copy) {
    try {
      await navigator.clipboard.writeText(copy.dataset.copySpace);
      toast('Space reference copied.');
    } catch {
      toast(copy.dataset.copySpace);
    }
  }
  const settings = event.target.closest('[data-space-list-settings]');
  if (settings) {
    settings.closest('details')?.removeAttribute('open');
    openListSettings(settings.dataset.spaceListSettings);
  }
});
