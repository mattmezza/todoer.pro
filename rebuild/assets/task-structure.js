import { setSaveState, trackSave } from './quiet-info.js';
async function request(id, body, endpoint = 'subtree/query') {
  const response = await fetch(`/api/v1/tasks/${encodeURIComponent(id)}/${endpoint}`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-CSRF-Token': document.querySelector('meta[name=csrf-token]')?.content || '',
    },
    body: JSON.stringify(body),
    cache: 'no-store',
  });
  const value = await response.json();
  if (!response.ok) throw Error(value.error?.message || 'Could not load task structure.');
  return value;
}
const projection = (cursor) => ({
  limit: 100,
  fields: ['title', 'complete', 'revision'],
  cursor: cursor || null,
});
function tree(items, currentId) {
  const byParent = new Map(),
    ids = new Set(items.map((item) => item.id));
  for (const item of items) {
    const key = ids.has(item.parent_id) ? item.parent_id : null;
    if (!byParent.has(key)) byParent.set(key, []);
    byParent.get(key).push(item);
  }
  function branch(parent) {
    const list = document.createElement('ul');
    for (const item of byParent.get(parent) || []) {
      const row = document.createElement('li');
      const link = document.createElement('a');
      link.href = `/tasks/${encodeURIComponent(item.id)}`;
      const label = document.createElement('span');
      label.className = 'structure-task-title';
      label.textContent = item.title || 'Untitled task';
      link.append(label);
      if (currentId) {
        const mark = document.createElement('span');
        mark.className = 'structure-task-mark';mark.setAttribute('aria-hidden', 'true');
        if (item.complete) mark.textContent = '✓';
        link.prepend(mark);
      }
      if (item.id === currentId) {
        const current = document.createElement('small');current.textContent = 'Current task';current.className = 'structure-current-label';link.append(current);
      }

      if (item.complete) link.classList.add('is-complete');
      if (item.id === currentId) link.setAttribute('aria-current', 'page');
      row.append(link);
      if (byParent.has(item.id)) row.append(branch(item.id));
      list.append(row);
    }
    return list;
  }
  const result = branch(null);
  result.className = 'task-structure-tree';
  return result;
}
export function mountStructure(container, id) {
  let alive = true,
    items = [],
    rootId,
    snapshot,
    cursor;
  const status = document.createElement('p');
  status.role = 'status';
  status.className = 'muted';
  status.textContent = 'Loading hierarchy…';
  const view = document.createElement('div');
  view.className = 'task-structure-view';
  const heading = document.createElement('h3');heading.textContent = 'Task hierarchy';
  const more = document.createElement('button');
  more.type = 'button';
  more.className = 'activity-more';
  more.textContent = 'Show more';
  more.hidden = true;
  container.append(heading, status, view, more);
  async function load(first = false) {
    more.disabled = true;
    try {
      if (first) {
        const context = await request(id, projection());
        if (!alive) return;
        rootId = context.context.breadcrumbs[0]?.id || id;
        status.textContent = context.context.truncated
          ? 'Showing the nearest available ancestors.'
          : '';
      }
      const page = await request(rootId, projection(cursor));
      if (!alive) return;
      if (snapshot && JSON.stringify(snapshot) !== JSON.stringify(page.snapshot))
        throw Error('The hierarchy changed. Reopen Structure to refresh.');
      snapshot = page.snapshot;
      items.push(...page.items);
      cursor = page.next_cursor;
      view.replaceChildren(tree(items, id));
      more.hidden = !cursor;
    } catch (error) {
      if (alive) status.textContent = error.message;
    } finally {
      more.disabled = false;
    }
  }
  more.onclick = () => load();
  load(true);
  return () => {
    alive = false;
  };
}
let moveOpen = false;
export async function moveToParent(id) {
  if (moveOpen) return;
  moveOpen = true;
  const source = `structure:${id}`;
  let dialog;
  try {
    const first = await request(id, projection());
    const selected = first.items[0];
    if (!selected?.parent_id) throw Error('This task is already top-level.');
    const breadcrumbs = first.context.breadcrumbs;
    const currentParent = breadcrumbs.at(-2);
    if (!currentParent || currentParent.id !== selected.parent_id)
      throw Error('The parent is unavailable. Refresh and try again.');
    const destinationId = currentParent.parent_id;
    const destination = destinationId
      ? breadcrumbs.find((item) => item.id === destinationId)
      : null;
    if (destinationId && !destination)
      throw Error('The destination is unavailable. Refresh and try again.');
    let items = [...first.items],
      cursor = first.next_cursor;
    while (cursor) {
      const page = await request(id, projection(cursor));
      if (JSON.stringify(page.snapshot) !== JSON.stringify(first.snapshot))
        throw Error('The hierarchy changed. Review the move again.');
      items.push(...page.items);
      cursor = page.next_cursor;
    }
    dialog = document.createElement('dialog');
    dialog.className = 'quick-dialog task-promote-dialog';
    dialog.setAttribute('aria-labelledby', 'task-promote-heading');
    const title = document.createElement('h2');
    title.id = 'task-promote-heading';
    title.textContent = 'Move to parent';
    const description = document.createElement('p');
    description.textContent = `Move “${selected.title}” and ${items.length - 1} descendant${items.length === 2 ? '' : 's'} out of “${currentParent.title || 'its current parent'}”? They will be appended ${destination ? `under “${destination.title}”` : 'at the top level of this list'}.`;
    const hierarchy = document.createElement('div');
    hierarchy.className = 'task-promote-tree';
    const destinationLabel = document.createElement('strong');
    destinationLabel.textContent = destination?.title || 'List · top-level tasks';
    const explanation = document.createElement('p');
    explanation.textContent = 'The new structure will look like this:';
    const preview = tree(items, null);
    for (const link of preview.querySelectorAll('a')) {
      const label = document.createElement('span');
      label.textContent = link.textContent;
      label.className = link.className;
      link.replaceWith(label);
    }
    hierarchy.append(explanation, destinationLabel, preview);
    const actions = document.createElement('div');
    actions.className = 'confirmation-actions';
    const cancel = document.createElement('button');
    cancel.type = 'button';
    cancel.textContent = 'Cancel';
    cancel.onclick = () => dialog.close();
    const confirm = document.createElement('button');
    confirm.type = 'button';
    confirm.className = 'primary';
    confirm.textContent = 'Move to parent';
    confirm.onclick = async () => {
      confirm.disabled = true;
      cancel.disabled = true;
      try {
        await trackSave(source, () =>
          request(id, { snapshot: first.snapshot }, 'promote'),
        );
        dialog.close();
        location.reload();
      } catch (error) {
        setSaveState(source, 'error', error.message);
        confirm.disabled = false;
        cancel.disabled = false;
      }
    };
    actions.append(cancel, confirm);
    dialog.append(title, description, hierarchy, actions);
    document.body.append(dialog);
    dialog.addEventListener(
      'close',
      () => {
        moveOpen = false;
        dialog.remove();
      },
      { once: true },
    );
    dialog.showModal();
    cancel.focus();
  } catch (error) {
    moveOpen = false;
    setSaveState(source, 'error', error.message);
  }
}
document.addEventListener('click', (event) => {
  const button = event.target.closest('[data-task-promote]');
  if (!button) return;
  event.preventDefault();
  const menu = button.closest('details');
  if (menu) menu.open = false;
  if (!button.disabled) moveToParent(button.dataset.taskPromote);
});
