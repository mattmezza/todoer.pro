import { inference } from './ai-transaction.js';
import { beginLoading } from './loading.js';
import { request } from './request.js';
export function mountBreakdown(root, { inline = false } = {}) {
  if (!root || root.dataset.canWrite !== 'true') return;
  const menu = root.querySelector('.task-context-menu');
  const dialog = document.createElement('dialog');
  dialog.className = 'breakdown-dialog';
  dialog.setAttribute('aria-labelledby', 'breakdown-title');
  const heading = document.createElement('h2');
  heading.id = 'breakdown-title';
  heading.innerHTML =
    '<span class="modal-heading-icon" aria-hidden="true"><svg class="ui-icon" width="20" height="20"><use href="/assets/icons.svg#sparkles"/></svg></span>Break down task';
  const close = document.createElement('button');
  close.type = 'button';
  close.className = 'workbench-dialog-close';
  close.innerHTML =
    '<svg class="ui-icon" width="20" height="20" aria-hidden="true"><use href="/assets/icons.svg#close"/></svg>';
  close.setAttribute('aria-label', 'Close breakdown');
  close.onclick = () => dialog.close();
  let applying = false,
    generation = 0,
    pending = false;
  dialog.addEventListener('cancel', (event) => {
    if (applying) event.preventDefault();
  });
  dialog.addEventListener('close', () => {
    // Keep in-flight results available when the dialog is reopened.
  });
  const button = document.createElement('button');
  button.type = 'button';
  button.innerHTML =
    '<svg class="ui-icon" width="20" height="20" aria-hidden="true"><use href="/assets/icons.svg#sparkles"/></svg><span>Break down…</span>';
  const explanation = document.createElement('p');
  explanation.className = 'muted';
  explanation.textContent =
    'On request, your saved task title, description, and global AI instructions are sent to the configured OpenRouter provider. Review suggestions before adding them.';
  const status = document.createElement('p');
  status.setAttribute('role', 'status');
  const box = document.createElement('div');
  box.className = 'breakdown-preview';
  button.hidden = true;
  let inlineButton;
  if (inline) {
    inlineButton = document.createElement('button');
    inlineButton.type = 'button';
    inlineButton.className = 'panel-breakdown-action';
    inlineButton.innerHTML =
      '<svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#sparkles"/></svg><span>AI breakdown…</span>';
    inlineButton.hidden = true;
    inlineButton.onclick = () => button.click();
    root.querySelector('[data-child-composer]')?.after(inlineButton);
  }
  request('/api/v1/assist/status')
    .then((r) => r.json())
    .then((state) => {
      button.hidden = !state.enabled;
      if (inlineButton) inlineButton.hidden = !state.enabled;
    })
    .catch(() => {});
  menu.querySelector('div').append(button);
  dialog.append(close, heading, explanation, status, box);
  root.append(dialog);
  let preview, entries, applyKey;
  const call = async (path, payload) => {
    if (path === 'drafts' || path === 'classify')
      return inference(path, payload, root.dataset.csrf);
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
      row.className = 'breakdown-row';
      row.classList.toggle('is-excluded', !entry.selected);
      const summary = document.createElement('div');
      summary.className = 'breakdown-summary';
      const title = document.createElement('strong');
      title.textContent = entry.title;
      const description = document.createElement('p');
      description.textContent = entry.description;
      summary.append(title, description);
      const controls = document.createElement('div');
      controls.className = 'breakdown-controls';
      const edit = document.createElement('button');
      edit.type = 'button';
      edit.textContent = entry.editing ? 'Done' : 'Edit';
      edit.setAttribute('aria-expanded', String(!!entry.editing));
      edit.onclick = () => {
        entry.editing = !entry.editing;
        draw();
      };
      const exclude = document.createElement('button');
      exclude.type = 'button';
      exclude.textContent = entry.selected ? 'Exclude' : 'Include';
      exclude.setAttribute('aria-pressed', String(!entry.selected));
      exclude.onclick = () => {
        entry.selected = !entry.selected;
        applyKey = null;
        draw();
      };
      controls.append(edit, exclude);
      row.append(summary, controls);
      if (entry.editing) {
        const editor = document.createElement('div');
        editor.className = 'breakdown-editor';
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
            title.textContent = entry.title;
            description.textContent = entry.description;
            applyKey = null;
          };
          label.append(field);
          editor.append(label);
        }
        row.append(editor);
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
    const actions = document.createElement('div');
    actions.className = 'breakdown-actions';
    actions.append(cancel, apply);
    box.append(actions);
  };
  button.onclick = async () => {
    menu.open = false;
    dialog.showModal();
    if (pending || entries) return;
    pending = true;
    box.replaceChildren();
    status.textContent = 'Finding a practical first step…';
    const messages = [
      'Turning your task into manageable steps…',
      'Still working. Longer tasks can take a little while…',
      'Your suggestions will appear here when they are ready.',
    ];
    let messageIndex = 0;
    const progress = setInterval(() => {
      status.textContent = messages[Math.min(messageIndex++, messages.length - 1)];
    }, 10000);
    const skeleton = beginLoading(box, { variant: 'form' });
    const requestGeneration = ++generation;
    try {
      const result = await call('drafts', {
        idempotency_key: crypto.randomUUID(),
        brief: 'Task breakdown',
        target: { kind: 'existing_task', task_id: root.dataset.taskId },
      });
      if (requestGeneration !== generation) return;
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
      if (requestGeneration === generation) status.textContent = error.message;
    } finally {
      clearInterval(progress);
      pending = false;
      skeleton.finish();
    }
  };
}
