import { request as fetch } from './request.js';
import { editorAnchor, alignEditorField } from './seamless-editor.js';
import { toast } from './toast.js';
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
});

const root = document.querySelector('[data-space-id]');
const form = root?.querySelector('[data-space-edit-form]');
const heading = root?.querySelector('[data-space-display]');
const description = root?.querySelector('.space-overview-description');
function editSpace(target) {
  if (!form || !form.hidden) return;
  const titleAnchor = editorAnchor(heading.querySelector('h1'));
  const descriptionAnchor = editorAnchor(description);
  heading.querySelector('.entity-menu').open = false;
  form.hidden = false;
  heading.hidden = true;
  description.hidden = true;
  const title = form.querySelector('[name="title"]');
  const brief = form.querySelector('[name="description"]');
  alignEditorField(title, titleAnchor, { expand: true });
  alignEditorField(brief, descriptionAnchor);
  const input = target.closest('.space-overview-description') ? brief : title;
  input.focus();
  if (input === title) title.select();
}
root?.addEventListener('click', (event) => {
  if (event.target.closest('[data-space-edit],[data-space-edit-open]'))
    editSpace(event.target);
  if (event.target.closest('[data-space-edit-cancel]')) {
    form.hidden = true;
    heading.hidden = false;
    description.hidden = false;
  }
});
root?.addEventListener('keydown', (event) => {
  if (['Enter', ' '].includes(event.key) && event.target.matches('[data-space-edit]')) {
    event.preventDefault();
    editSpace(event.target);
  }
});
form?.addEventListener('submit', async (event) => {
  event.preventDefault();
  const save = form.querySelector('[type="submit"]');
  if (save.disabled) return;
  save.disabled = true;
  try {
    const data = new FormData(form);
    const response = await fetch(
      `/api/v1/workspaces/${encodeURIComponent(root.dataset.spaceId)}`,
      {
        method: 'PATCH',
        headers: {
          'Content-Type': 'application/json',
          'X-CSRF-Token': document.querySelector('meta[name=csrf-token]').content,
        },
        body: JSON.stringify({
          revision: Number(root.dataset.spaceRevision),
          title: data.get('title'),
          description: data.get('description'),
        }),
      },
    );
    if (!response.ok) {
      const data = await response.json().catch(() => null);
      throw Error(
        data?.error?.message || 'Could not save. Refresh the page and try again',
      );
    }
    location.reload();
  } catch (error) {
    form.querySelector('[role="alert"]').textContent =
      `${error.message}. Your draft is unchanged.`;
  } finally {
    save.disabled = false;
  }
});

for (const card of document.querySelectorAll('.space-list-card')) {
  const dot = card.querySelector('.list-dot');
  if (dot)
    card.style.setProperty(
      '--list-color',
      dot.dataset.entityColor || getComputedStyle(dot).backgroundColor,
    );
}

const archivedToggle = document.querySelector('[data-show-archived]');
archivedToggle?.addEventListener('change', () => {
  document.querySelector('[data-archived-lists]').hidden = !archivedToggle.checked;
});
