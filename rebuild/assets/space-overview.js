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
function editSpace(display) {
  if (root.querySelector('[data-space-edit-form]')) return;
  const anchor = editorAnchor(display);
  const field = display.dataset.spaceEdit;
  const form = document.createElement('form');
  form.dataset.spaceEditForm = '';
  form.className = 'space-inline-editor';
  const input = document.createElement('textarea');
  input.value =
    field === 'description' && display.textContent === 'Add a description'
      ? ''
      : display.textContent;
  input.name = field;
  input.required = field === 'title';
  input.maxLength = field === 'title' ? 500 : 100000;
  input.rows = 1;
  const cancel = document.createElement('button');
  cancel.type = 'button';
  cancel.textContent = 'Cancel';
  const save = document.createElement('button');
  save.type = 'submit';
  save.textContent = 'Save';
  save.className = 'primary';
  const status = document.createElement('p');
  status.role = 'status';
  const actions = document.createElement('div');
  actions.append(cancel, save);
  form.append(input, actions, status);
  display.hidden = true;
  display.after(form);
  const close = () => {
    form.remove();
    display.hidden = false;
  };
  cancel.onclick = close;
  form.onkeydown = (e) => {
    if (e.key === 'Escape') close();
  };
  form.onsubmit = async (e) => {
    e.preventDefault();
    save.disabled = true;
    try {
      const response = await fetch(
        '/api/v1/workspaces/' + encodeURIComponent(root.dataset.spaceId),
        {
          method: 'PATCH',
          headers: {
            'Content-Type': 'application/json',
            'X-CSRF-Token': document.querySelector('meta[name=csrf-token]').content,
          },
          body: JSON.stringify({
            revision: Number(root.dataset.spaceRevision),
            [field]: input.value,
          }),
        },
      );
      if (!response.ok) throw Error('Could not save. Refresh the page and try again.');
      const value = await response.json();
      root.dataset.spaceRevision = value.revision;
      display.textContent = value[field] || 'Add a description';
      close();
    } catch (error) {
      status.textContent = error.message;
      save.disabled = false;
    }
  };
  alignEditorField(input, anchor);
  input.focus();
  input.select();
}
root?.addEventListener('click', (e) => {
  const display = e.target.closest('[data-space-edit]');
  if (display) editSpace(display);
});
root?.addEventListener('keydown', (e) => {
  if (['Enter', ' '].includes(e.key) && e.target.matches('[data-space-edit]')) {
    e.preventDefault();
    editSpace(e.target);
  }
});
