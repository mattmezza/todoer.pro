import { confirmAction } from './confirm.js';

// Read the authorized subtree, including descendants hidden by list filters or
// pagination. Keep the caller's revision so concurrent edits still fail CAS.
export async function taskCompletionChange(api, task, forceSubtree = false) {
  const change = {
    revision: task.revision,
    complete: forceSubtree || !task.complete,
    complete_subtree: false,
  };
  if (!change.complete) return change;
  let cursor = null;
  let unfinished = false;
  do {
    const page = await api(
      `/tasks/${encodeURIComponent(task.id)}/subtree/query`,
      'POST',
      {
        limit: 100,
        fields: ['complete'],
        cursor,
      },
    );
    for (const item of page.items) {
      if (item.id === task.id) continue;
      change.complete_subtree = true;
      if (!item.complete) unfinished = true;
    }
    cursor = page.next_cursor;
  } while (cursor && !unfinished);
  if (
    (unfinished || forceSubtree) &&
    !(await confirmAction(
      'Complete this task and every unfinished sub-task? This includes sub-tasks hidden by filters.',
    ))
  )
    return null;
  change.complete_subtree ||= forceSubtree;
  return change;
}
