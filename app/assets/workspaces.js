import { request as fetch } from './request.js';
import './site.js';
document.addEventListener('click', (event) => {
  const button = event.target.closest('[data-workspace-action]');
  if (!button) return;
  const row = button.closest('[data-workspace]'),
    action = button.dataset.workspaceAction;
  const dialog = document.createElement('dialog');
  dialog.className = 'quick-dialog confirmation-dialog';
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
  submit.className = action === 'restore' ? 'primary' : 'primary destructive';
  submit.textContent =
    action === 'delete' ? 'Continue' : action === 'archive' ? 'Archive' : 'Restore';
  error.setAttribute('role', 'alert');
  const input = document.createElement('input');
  input.placeholder = 'Type the exact workspace name';
  input.setAttribute('aria-label', 'Workspace name confirmation');
  input.autocomplete = 'off';
  input.hidden = true;
  let stage = 0;
  const actions = document.createElement('div');
  actions.className = 'confirmation-actions';
  actions.append(cancel, submit);
  form.append(input, actions, error);
  dialog.append(heading, text, form);
  document.body.append(dialog);
  dialog.addEventListener('close', () => dialog.remove());
  dialog.showModal();
  form.onsubmit = async (event) => {
    event.preventDefault();
    if (submit.disabled) return;
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

const createDialog = document.querySelector('#workspace-create-dialog');
document.addEventListener('click', (event) => {
  if (event.target.closest('[data-workspace-create-open]') && createDialog) {
    event.preventDefault();
    createDialog.showModal();
    createDialog.querySelector('[name=title]').focus();
  }
  if (event.target.closest('[data-workspace-create-close]')) createDialog?.close();
});
document.body.dataset.workspacesReady = 'true';

for (const button of document.querySelectorAll('[data-workspace-action]')) {
  const action = button.dataset.workspaceAction;
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  const use = document.createElementNS('http://www.w3.org/2000/svg', 'use');
  svg.classList.add('ui-icon');
  svg.setAttribute('aria-hidden', 'true');
  use.setAttribute(
    'href',
    `/assets/icons.svg#${action === 'archive' ? 'archive-box' : action === 'restore' ? 'arrow-uturn-left' : 'trash'}`,
  );
  svg.append(use);
  button.prepend(svg);
}
document.addEventListener('click', (event) => {
  const row = event.target.closest('.workspace-row[data-workspace]');
  if (
    !row ||
    event.target.closest('a,button,input,select,textarea,summary') ||
    window.getSelection()?.toString() ||
    event.button !== 0
  )
    return;
  row.querySelector('.workspace-row-title')?.click();
});
document
  .querySelector('.space-archive-filter input')
  ?.addEventListener('change', (event) => event.target.form.requestSubmit());
