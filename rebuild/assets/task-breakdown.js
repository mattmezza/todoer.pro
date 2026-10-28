import { request } from './request.js';
export function mountBreakdown(root) {
  if (!root || root.dataset.canWrite !== 'true') return;
  const menu = root.querySelector('.task-context-menu');
  const dialog = document.createElement('dialog');
  dialog.className = 'breakdown-dialog';
  dialog.setAttribute('aria-labelledby', 'breakdown-title');
  const heading = document.createElement('h2');
  heading.id = 'breakdown-title';
  heading.textContent = 'Break down task';
  const close = document.createElement('button');
  close.type = 'button';
  close.textContent = 'Close';
  close.setAttribute('aria-label', 'Close breakdown');
  close.onclick = () => dialog.close();
  let applying = false,
    generation = 0;
  dialog.addEventListener('cancel', (event) => {
    if (applying) event.preventDefault();
  });
  dialog.addEventListener('close', () => {
    generation++;
  });
  const button = document.createElement('button');
  button.type = 'button';
  button.textContent = 'Break down…';
  const explanation = document.createElement('p');
  explanation.className = 'muted';
  explanation.textContent =
    'On request, your saved task title, description, and global AI instructions are sent to the configured OpenRouter provider. Review suggestions before adding them.';
  const status = document.createElement('p');
  status.setAttribute('role', 'status');
  const box = document.createElement('div');
  menu.querySelector('div').append(button);
  dialog.append(close, heading, explanation, status, box);
  root.append(dialog);
  let preview, entries, applyKey;
  const call = async (path, payload) => {
    const response = await request('/api/v1/assist/' + path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': root.dataset.csrf },
      body: JSON.stringify(payload),
    });
    const body = await response.json();
    if (!response.ok) throw Error(body.error?.message || 'Request failed. Try again.');
    return body;
  };
  const draw = () => {
    box.replaceChildren();
    const heading = document.createElement('h3');
    heading.textContent = 'Review suggested sub-tasks';
    box.append(heading);
    entries.forEach((entry, index) => {
      const row = document.createElement('fieldset');
      const legend = document.createElement('legend');
      legend.textContent = `Suggestion ${index + 1}`;
      row.append(legend);
      const selected = document.createElement('input');
      selected.type = 'checkbox';
      selected.checked = entry.selected;
      selected.setAttribute('aria-label', 'Include suggestion');
      selected.onchange = () => {
        entry.selected = selected.checked;
        applyKey = null;
      };
      row.append(selected);
      for (const [key, labelText] of [
        ['title', 'Title'],
        ['description', 'Description'],
      ]) {
        const label = document.createElement('label');
        label.textContent = labelText;
        const field = document.createElement(key === 'title' ? 'input' : 'textarea');
        field.value = entry[key];
        field.maxLength = key === 'title' ? 500 : 2000;
        field.oninput = () => {
          entry[key] = field.value;
          applyKey = null;
        };
        label.append(field);
        row.append(label);
      }
      for (const [label, delta] of [
        ['Move up', -1],
        ['Move down', 1],
      ]) {
        const b = document.createElement('button');
        b.type = 'button';
        b.textContent = label;
        b.disabled = index + delta < 0 || index + delta >= entries.length;
        b.onclick = () => {
          [entries[index], entries[index + delta]] = [
            entries[index + delta],
            entries[index],
          ];
          applyKey = null;
          draw();
        };
        row.append(b);
      }
      box.append(row);
    });
    const apply = document.createElement('button');
    apply.type = 'button';
    apply.textContent = 'Add selected sub-tasks';
    apply.className = 'primary';
    apply.onclick = async () => {
      apply.disabled = true;
      close.disabled = true;
      applying = true;
      status.textContent = '';
      try {
        const tasks = entries
          .filter((e) => e.selected)
          .map(({ title, description }) => ({
            title: title.trim(),
            description,
            parent_index: null,
          }));
        if (!tasks.length || tasks.some((t) => !t.title))
          throw Error('Select at least one sub-task and give each a title.');
        if (!applyKey) {
          preview = await call(`drafts/${encodeURIComponent(preview.id)}/review`, {
            revision: preview.revision,
            payload: { target: preview.payload.target, tasks },
          });
          applyKey = crypto.randomUUID();
        }
        await call('apply', {
          preview_id: preview.id,
          revision: preview.revision,
          expected_hash: preview.expected_hash,
          idempotency_key: applyKey,
        });
        location.reload();
      } catch (error) {
        status.textContent = error.message;
      } finally {
        applying = false;
        close.disabled = false;
        apply.disabled = false;
      }
    };
    const cancel = document.createElement('button');
    cancel.type = 'button';
    cancel.textContent = 'Cancel';
    cancel.onclick = () => {
      if (!applying) dialog.close();
    };
    box.append(apply, cancel);
  };
  button.onclick = async () => {
    menu.open = false;
    box.replaceChildren();
    status.textContent = 'Generating suggestions…';
    dialog.showModal();
    const requestGeneration = ++generation;
    try {
      const result = await call('drafts', {
        idempotency_key: crypto.randomUUID(),
        brief: 'Task breakdown',
        target: { kind: 'existing_task', task_id: root.dataset.taskId },
      });
      if (!dialog.open || requestGeneration !== generation) return;
      if (result.status !== 'draft')
        throw Error(
          'AI assistance is unavailable. You can still add sub-tasks manually.',
        );
      preview = result.preview;
      entries = preview.payload.tasks.map((t) => ({
        title: t.title,
        description: t.description,
        selected: true,
      }));
      applyKey = null;
      draw();
      status.textContent = '';
    } catch (error) {
      if (dialog.open && requestGeneration === generation)
        status.textContent = error.message;
    }
  };
}
