import { request as fetch } from './request.js';
const csrf = document.querySelector('meta[name="csrf-token"]').content;
document.addEventListener('click', async (e) => {
  const button = e.target.closest('.restore,.undo');
  if (!button) return;
  const path = button.matches('.restore')
    ? `tasks/${button.dataset.id}/restore`
    : `history/${button.dataset.seq}/undo`;
  button.disabled = true;
  const notice = document.querySelector('#notice');
  try {
    const response = await fetch(`/api/v1/${path}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': csrf },
      body: JSON.stringify({ revision: Number(button.dataset.revision) }),
    });
    const data = await response.json();
    if (!response.ok) throw new Error(data.error?.message || 'Operation failed');
    if (button.matches('.restore')) { location.reload(); return; }
    button.closest('article').remove();
    notice.textContent = button.matches('.restore') ? 'Task restored.' : 'Edit undone.';
  } catch (err) {
    notice.textContent = err.message;
    notice.classList.add('error');
    button.disabled = false;
  }
});
