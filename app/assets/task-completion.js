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

export function setCompletionIcon(button, complete) {
  button.replaceChildren();
  if (!complete) return;
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('class', 'ui-icon');
  svg.setAttribute('width', '14');
  svg.setAttribute('height', '14');
  svg.setAttribute('aria-hidden', 'true');
  const use = document.createElementNS('http://www.w3.org/2000/svg', 'use');
  use.setAttribute('href', '/assets/icons.svg#check');
  svg.append(use);
  button.append(svg);
}

export function animateTaskCompletion(control, complete) {
  if (!control || matchMedia('(prefers-reduced-motion: reduce)').matches) return;
  const frames = complete
    ? [{ transform: 'scale(.88)' }, { transform: 'scale(1.12)', offset: 0.55 }]
    : [{ transform: 'scale(1.05)' }, { transform: 'scale(.86)', offset: 0.4 }];
  return control
    .animate([...frames, { transform: 'scale(1)' }], {
      duration: 170,
      easing: 'ease-out',
    })
    .finished.catch(() => {});
}
