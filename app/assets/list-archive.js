import { request as fetch } from './request.js';
import { confirmAction } from './confirm.js';
import { toast } from './toast.js';
const restoreButton = document.querySelector('[data-list-restore]');
if (restoreButton) restoreButton.disabled = false;
restoreButton?.addEventListener('click', async (event) => {
  const button = event.currentTarget;
  button.disabled = true;
  const confirmed = await confirmAction(
    'Restoring this list makes it active and editable again. It will appear with active lists in its space, and its tasks can appear in search results, planning views and other active task views. Existing members and direct shares keep their access.',
    { title: 'Restore this list?', acceptLabel: 'Restore list' },
  );
  if (!confirmed) {
    button.disabled = false;
    button.focus();
    return;
  }
  try {
    const response = await fetch(`/api/v1/lists/${button.dataset.listId}`, {
      method: 'PATCH',
      headers: {
        'Content-Type': 'application/json',
        'X-CSRF-Token': document.querySelector('meta[name=csrf-token]').content,
      },
      body: JSON.stringify({
        revision: Number(button.dataset.revision),
        archived: false,
      }),
    });
    if (!response.ok)
      throw Error((await response.json()).error?.message || 'Could not restore list.');
    location.reload();
  } catch (error) {
    toast(error.message);
    button.disabled = false;
  }
});
