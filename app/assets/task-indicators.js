// Quiet counts share one row slot and keep the existing row interaction.
export function taskIndicators(comments, subtasks) {
  const group = document.createElement('span');
  group.className = 'task-indicators';
  for (const [count, kind, icon] of [
    [subtasks, 'subtask', 'subtasks'],
    [comments, 'comment', 'comment'],
  ]) {
    if (!(count > 0)) continue;
    const indicator = document.createElement('span');
    indicator.className = `task-row-count task-${kind}-count`;
    indicator.setAttribute('role', 'img');
    const label = `${count} ${kind}${count === 1 ? '' : 's'}`;
    indicator.setAttribute('aria-label', label);
    indicator.title = label;
    indicator.innerHTML = `<svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#${icon}"/></svg>`;
    const number = document.createElement('span');
    number.setAttribute('aria-hidden', 'true');
    number.textContent = String(count);
    indicator.append(number);
    group.append(indicator);
  }
  return group.childElementCount ? group : null;
}
