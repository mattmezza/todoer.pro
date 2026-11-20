// Pending rows have no task ID so normal task controls cannot edit or move them.
export function pendingTaskRow(tag, title, description) {
  const row = document.createElement(tag);
  row.className = 'pending-task';
  const check = document.createElement('span');
  check.className = 'pending-task-check';
  check.setAttribute('aria-hidden', 'true');
  const copy = document.createElement('div');
  const name = document.createElement('span');
  name.className = 'task-title';
  name.textContent = title;
  const state = document.createElement('small');
  state.className = 'pending-task-state';
  state.textContent = 'Saving…';
  copy.append(name, state);
  if (description) {
    const text = document.createElement('div');
    text.className = 'pending-task-description';
    text.textContent = description;
    copy.append(text);
  }
  row.append(check, copy);
  return row;
}

export function animateSavedTask(row) {
  if (row && !matchMedia('(prefers-reduced-motion: reduce)').matches)
    row.animate(
      [{ backgroundColor: 'var(--hover)' }, { backgroundColor: 'transparent' }],
      { duration: 220, easing: 'ease-out' },
    );
}
