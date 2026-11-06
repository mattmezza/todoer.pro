import './site.js';
import { onPageLoad } from './page-lifecycle.js';

onPageLoad((scope) => {
  if (!(
    document.querySelector('[data-workspace-create-open], [data-workspace-create]') ||
    location.pathname === '/spaces'
  ))
    return;
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
  scope.listen(document, 'click', (event) => {
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
  scope.listen(document.querySelector('.space-archive-filter input'), 'change', (event) =>
    event.target.form.requestSubmit(),
  );
});
