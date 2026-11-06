import { refreshPage } from './page-navigation.js';
import { beginLoading } from './loading.js';
import { request as fetch } from './request.js';
import { setSaveState, trackSave } from './quiet-info.js';
import { toast } from './toast.js';
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
        mark.className = 'structure-task-mark';
        mark.setAttribute('aria-hidden', 'true');
        if (item.complete) mark.textContent = '✓';
        link.prepend(mark);
      }
      if (item.id === currentId) {
        const current = document.createElement('small');
        current.textContent = 'Current task';
        current.className = 'structure-current-label';
        link.append(current);
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
  status.textContent = '';
  const view = document.createElement('div');
  view.className = 'task-structure-view';
  const heading = document.createElement('h3');
  heading.textContent = 'Task hierarchy';
  const more = document.createElement('button');
  more.type = 'button';
  more.className = 'activity-more';
  more.textContent = 'Show more';
  more.hidden = true;
  container.append(heading, status, view, more);
  async function load(first = false) {
    const skeleton = beginLoading(view, { variant: 'tree', preserve: items.length > 0 });
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
      if (alive) {
        status.textContent = error.message;
        skeleton.fail(error.message, () => load(first));
      }
    } finally {
      skeleton.finish();
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
  try {
    const first = await request(id, { ...projection(), limit: 1 });
    if (!first.items[0]?.parent_id) throw Error('This task is already top-level.');
    await trackSave(source, () => request(id, { snapshot: first.snapshot }, 'promote'));
    await refreshPage();
    toast('Task moved to parent.');
  } catch (error) {
    setSaveState(source, 'error', error.message);
  } finally {
    moveOpen = false;
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
