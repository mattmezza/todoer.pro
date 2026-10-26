import './site.js';
document.addEventListener('click', (event) => {
  const button = event.target.closest('[data-workspace-action]');
  if (!button) return;
  const row = button.closest('[data-workspace]'),
    action = button.dataset.workspaceAction;
  const dialog = document.createElement('dialog');
  dialog.className = 'quick-dialog';
  const heading = document.createElement('h2');
  heading.textContent =
    action === 'delete'
      ? 'Delete archived workspace?'
      : action === 'archive'
        ? 'Archive workspace?'
        : 'Restore workspace?';
  const text = document.createElement('p');
  text.textContent =
    action === 'delete'
      ? 'This removes the workspace from use and revokes access to its lists. Data and files are retained in trash. Continue to confirm its name.'
      : 'Tasks and history are preserved.';
  const form = document.createElement('form'),
    cancel = document.createElement('button'),
    submit = document.createElement('button'),
    error = document.createElement('p');
  cancel.type = 'button';
  cancel.textContent = 'Cancel';
  cancel.onclick = () => dialog.close();
  submit.type = 'submit';
  submit.className = 'primary';
  submit.textContent =
    action === 'delete' ? 'Continue' : action === 'archive' ? 'Archive' : 'Restore';
  error.setAttribute('role', 'alert');
  const input = document.createElement('input');
  input.placeholder = 'Type the exact workspace name';
  input.setAttribute('aria-label', 'Workspace name confirmation');
  input.autocomplete = 'off';
  input.hidden = true;
  let stage = 0;
  form.append(input, cancel, submit, error);
  dialog.append(heading, text, form);
  document.body.append(dialog);
  dialog.addEventListener('close', () => dialog.remove());
  dialog.showModal();
  form.onsubmit = async (event) => {
    event.preventDefault();
    if (action === 'delete' && stage === 0) {
      stage = 1;
      input.hidden = false;
      input.required = true;
      submit.textContent = 'Delete workspace';
      text.textContent = `Type “${row.dataset.title}” to confirm.`;
      input.focus();
      return;
    }
    submit.disabled = true;
    try {
      const response = await fetch(
        `/api/v1/workspaces/${row.dataset.workspace}/lifecycle`,
        {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            'X-CSRF-Token': document.querySelector('meta[name="csrf-token"]').content,
          },
          body: JSON.stringify({
            revision: Number(row.dataset.revision),
            action,
            confirmation: action === 'delete' ? input.value : null,
          }),
        },
      );
      if (!response.ok)
        throw Error(
          (await response.json()).error?.message || 'Could not update workspace',
        );
      location.reload();
    } catch (e) {
      error.textContent = e.message;
      submit.disabled = false;
    }
  };
});

document.body.dataset.workspacesReady = 'true';
