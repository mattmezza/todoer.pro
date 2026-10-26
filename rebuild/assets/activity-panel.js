import { confirmAction } from './confirm.js';
export function mountActivity(container, kind, id) {
  let active = true,
    controller = new AbortController();
  const heading = document.createElement('h3');
  heading.textContent = 'Activity';
  const message = document.createElement('p');
  message.role = 'status';
  message.className = 'muted';
  const entries = document.createElement('div');
  entries.className = 'activity-timeline';
  const more = document.createElement('button');
  more.type = 'button';
  more.textContent = 'Load earlier changes';
  more.className = 'activity-more';
  more.hidden = true;
  container.append(heading, message, entries, more);
  async function load(before) {
    message.textContent = 'Loading activity…';
    more.disabled = true;
    try {
      const response = await fetch(
        `/api/v1/${kind === 'workspace' ? 'workspaces' : 'tasks'}/${encodeURIComponent(id)}/activity${before ? `?before=${before}` : ''}`,
        { cache: 'no-store', signal: controller.signal },
      );
      if (!response.ok)
        throw Error(
          response.status === 403 || response.status === 404
            ? 'This activity is no longer accessible.'
            : 'Could not load activity. Try again.',
        );
      const page = await response.json();
      if (!active) return;
      for (const item of page.items) {
        const row = document.createElement('article');
        row.className = 'activity-item';
        const action = document.createElement('strong');
        action.textContent = item.kind.replaceAll('_', ' ').replaceAll('-', ' ');
        row.append(action);
        if (kind === 'workspace') {
          const link = document.createElement('a');
          link.href = `/tasks/${encodeURIComponent(item.task_id)}`;
          link.textContent = item.title;
          row.append(link);
        }
        const meta = document.createElement('p');
        meta.textContent = `${item.actor}${item.attribution === 'agent' ? ' · agent' : ''} · ${item.occurred_at ? new Date(item.occurred_at * 1000).toLocaleString() : 'Earlier change'}`;
        row.append(meta);
        if (item.undo_revision) {
          const undo = document.createElement('button');
          undo.type = 'button';
          undo.textContent = 'Undo edit';
          undo.onclick = async () => {
            if (
              !(await confirmAction(
                'Undo this edit? This is allowed only while its saved revision is still current.',
              ))
            )
              return;
            undo.disabled = true;
            try {
              const r = await fetch(`/api/v1/history/${item.sequence}/undo`, {
                method: 'POST',
                headers: {
                  'Content-Type': 'application/json',
                  'X-CSRF-Token':
                    document.querySelector('meta[name=csrf-token]')?.content || '',
                },
                body: JSON.stringify({ revision: item.undo_revision }),
              });
              if (!r.ok)
                throw Error(
                  (await r.json()).error?.message ||
                    'The task has changed. Refresh activity and review it.',
                );
              undo.remove();
              message.textContent = 'Edit undone.';
            } catch (error) {
              message.textContent = error.message;
              undo.disabled = false;
            }
          };
          row.append(undo);
        }
        entries.append(row);
      }
      message.textContent = entries.children.length ? '' : 'No recorded changes yet.';
      more.hidden = !page.next_before;
      more.onclick = () => load(page.next_before);
    } catch (error) {
      if (error.name !== 'AbortError' && active) message.textContent = error.message;
    } finally {
      more.disabled = false;
    }
  }
  load();
  return () => {
    active = false;
    controller.abort();
  };
}
