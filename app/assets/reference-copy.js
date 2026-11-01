import { toast } from './toast.js';

// Every entity reference uses the same copy behavior, including dynamic rows.
document.addEventListener('click', async (event) => {
  const button = event.target.closest(
    '[data-copy-reference],[data-copy-space],[data-copy-task-reference],[data-copy-child-reference]',
  );
  if (!button) return;
  const value =
    button.dataset.copyReference ??
    button.dataset.copySpace ??
    button.dataset.copyTaskReference ??
    button.dataset.copyChildReference;
  try {
    await navigator.clipboard.writeText(value);
    toast('Reference copied.');
  } catch {
    toast(`Copy reference: ${value}`);
  }
});
