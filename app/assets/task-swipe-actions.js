import { taskCompletionChange } from './task-completion.js';

// Shared mutations and Undo for list tasks and task-page subtasks.
export function createTaskSwipeActions({
  canWrite,
  api,
  notice,
  changed,
  getTask,
  beginWrite,
}) {
  let active = true;
  const swipeUndo = [];
  const swipingTasks = new Set();
  function showSwipeUndo() {
    for (const item of swipeUndo) item.element.remove();
    const latest = swipeUndo.at(-1);
    if (latest) document.body.append(latest.element);
  }
  async function swipeTask(row, action) {
    const task = getTask(row);
    if (!active || !task) return;
    const id = task.id;
    const kind = task.parent_id ? 'subtask' : 'task';
    if (!canWrite || swipingTasks.has(id) || (action === 'complete' && task.complete))
      return;
    swipingTasks.add(id);
    row.setAttribute('aria-busy', 'true');
    let release;
    try {
      release = beginWrite?.();
      const title = task.title || 'Task';
      const change =
        action === 'archive' ? { archive: true } : await taskCompletionChange(api, task);
      if (!change || !active) return;
      const result = await api(
        action === 'archive'
          ? `/cli/lifecycle/${kind}/${encodeURIComponent(id)}`
          : `/tasks/${encodeURIComponent(id)}`,
        action === 'archive' ? 'POST' : 'PATCH',
        change,
      );
      if (!active) return;
      const element = document.createElement('div');
      element.className = 'inbox-undo';
      element.setAttribute('role', 'status');
      const text = document.createElement('span');
      text.textContent = `${action === 'archive' ? 'Archived' : 'Completed'} · ${result.title || title}`;
      const undo = document.createElement('button');
      undo.type = 'button';
      undo.textContent = 'Undo';
      const item = { element };
      undo.onclick = async () => {
        undo.disabled = true;
        let release;
        try {
          release = beginWrite?.();
          await api(
            action === 'archive'
              ? `/cli/lifecycle/${kind}/${encodeURIComponent(id)}`
              : `/tasks/${encodeURIComponent(id)}`,
            action === 'archive' ? 'POST' : 'PATCH',
            action === 'archive'
              ? { archive: false }
              : { revision: result.revision, complete: false },
          );
          swipeUndo.splice(swipeUndo.indexOf(item), 1);
          element.remove();
          showSwipeUndo();
          if (active) changed();
        } catch (error) {
          if (active) notice(`${error.message} The task could not be restored.`, true);
          undo.disabled = false;
        } finally {
          release?.();
        }
      };
      element.append(text, undo);
      swipeUndo.push(item);
      showSwipeUndo();
      if (active) changed();
    } catch (error) {
      if (active) notice(`${error.message} The task was not changed.`, true);
      if (active && [409, 404, 403].includes(error.status)) changed();
    } finally {
      release?.();
      row.removeAttribute('aria-busy');
      swipingTasks.delete(id);
    }
  }

  swipeTask.destroy = () => {
    active = false;
    for (const item of swipeUndo) item.element.remove();
    swipeUndo.length = 0;
  };
  return swipeTask;
}
