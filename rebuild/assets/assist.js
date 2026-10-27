import { request as fetch } from './request.js';
const root = document.querySelector('[data-assist]');
if (root) {
  const error = root.querySelector('[data-error]');
  const call = async (path, body) => {
    const response = await fetch(`/api/v1/assist/${path}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': root.dataset.csrf },
      body: JSON.stringify(body),
    });
    const value = await response.json();
    if (!response.ok)
      throw new Error(
        value.error?.message || 'Request failed. Your input is still here.',
      );
    return value;
  };
  const run = async (button, action) => {
    button.disabled = true;
    error.textContent = '';
    try {
      await action();
    } catch (e) {
      error.textContent = e.message;
    } finally {
      button.disabled = false;
    }
  };
  let captureKey = crypto.randomUUID(),
    classifyKey = crypto.randomUUID(),
    draftKey = crypto.randomUUID(),
    applyKey = crypto.randomUUID(),
    captured,
    preview,
    reviewed = false;
  const capture = root.querySelector('[data-capture]'),
    classify = root.querySelector('[data-classify]'),
    draft = root.querySelector('[data-draft]'),
    review = root.querySelector('[data-review]'),
    apply = root.querySelector('[data-apply]');
  const result = root.querySelector('[data-draft-result]');
  capture.addEventListener('input', () => {
    captureKey = crypto.randomUUID();
  });
  capture.addEventListener('submit', (event) => {
    event.preventDefault();
    run(event.submitter, async () => {
      const data = new FormData(capture);
      const saved = await call('capture', {
        idempotency_key: captureKey,
        text: data.get('text'),
        list_id: data.get('list_id') || null,
        default_key: 'capture',
      });
      captured = saved.task;
      classifyKey = crypto.randomUUID();
      const link = document.createElement('a');
      link.href = `/tasks/${encodeURIComponent(captured.id)}`;
      link.textContent = `Saved: ${captured.title}`;
      root.querySelector('[data-capture-result]').replaceChildren(link);
      classify.hidden = false;
      root.querySelector('[data-suggestion]').textContent = '';
    });
  });
  classify.addEventListener('click', () =>
    run(classify, async () => {
      const value = await call('classify', {
        idempotency_key: classifyKey,
        task_id: captured.id,
        revision: captured.revision,
      });
      root.querySelector('[data-suggestion]').textContent =
        value.status === 'suggestion'
          ? value.destination
            ? `Suggested destination: ${value.destination.title}. Open the saved task to review a move. Nothing has moved.`
            : 'No clear destination suggested. Nothing has moved.'
          : 'Suggestion unavailable. Your task is already saved.';
    }),
  );
  draft.addEventListener('input', () => {
    draftKey = crypto.randomUUID();
  });
  const invalidate = () => {
    reviewed = false;
    apply.disabled = true;
    result.textContent = 'Changes need review before applying.';
  };
  const draw = () => {
    const container = root.querySelector('[data-draft-tasks]');
    container.replaceChildren();
    const titleLabel = root.querySelector('[data-reviewed-list-title]');
    titleLabel.hidden = preview.payload.target.kind !== 'new_list';
    if (!titleLabel.hidden) {
      const input = titleLabel.querySelector('input');
      input.value = preview.payload.target.title;
      input.oninput = () => {
        preview.payload.target.title = input.value;
        invalidate();
      };
    }
    preview.payload.tasks.forEach((task, index) => {
      const fieldset = document.createElement('fieldset'),
        legend = document.createElement('legend');
      legend.textContent = `Task ${index + 1}`;
      fieldset.append(legend);
      for (const [name, caption] of [
        ['title', 'Title'],
        ['description', 'Description'],
        ['parent_index', 'Parent task number'],
      ]) {
        const label = document.createElement('label');
        label.textContent = caption;
        const input = document.createElement(
          name === 'description' ? 'textarea' : 'input',
        );
        input.name = name;
        if (name === 'parent_index') {
          input.type = 'number';
          input.min = '1';
          input.max = String(index);
          input.value = task[name] === null ? '' : String(task[name] + 1);
        } else {
          input.value = task[name];
          input.maxLength = name === 'title' ? 500 : 10240;
          input.required = name === 'title';
        }
        input.addEventListener('input', () => {
          task[name] =
            name === 'parent_index'
              ? input.value === ''
                ? null
                : Number(input.value) - 1
              : input.value;
          invalidate();
        });
        label.append(input);
        fieldset.append(label);
      }
      container.append(fieldset);
    });
    review.hidden = false;
    apply.hidden = false;
    invalidate();
  };
  draft.addEventListener('submit', (event) => {
    event.preventDefault();
    run(event.submitter, async () => {
      const data = new FormData(draft);
      const target =
        data.get('target_kind') === 'new_list'
          ? {
              kind: 'new_list',
              workspace_id: data.get('workspace_id'),
              title: data.get('list_title'),
              description: '',
              restricted: data.get('restricted') === 'on',
            }
          : { kind: 'existing_list', list_id: data.get('list_id') };
      const value = await call('drafts', {
        idempotency_key: draftKey,
        brief: data.get('brief'),
        target,
      });
      if (value.status !== 'draft') {
        result.textContent =
          'Draft unavailable. Your brief is still here; you can capture it as a task.';
        return;
      }
      preview = value.preview;
      const suggestion = root.querySelector('[data-title-suggestion]');
      const useTitle = root.querySelector('[data-use-title]');
      const hasTitle =
        preview.payload.target.kind === 'new_list' &&
        typeof value.suggested_list_title === 'string';
      suggestion.hidden = !hasTitle;
      useTitle.hidden = !hasTitle;
      suggestion.textContent = hasTitle
        ? `Suggested title: ${value.suggested_list_title}`
        : '';
      useTitle.onclick = () => {
        preview.payload.target.title = value.suggested_list_title;
        root.querySelector('[data-reviewed-list-title] input').value =
          value.suggested_list_title;
        invalidate();
      };
      draw();
    });
  });
  review.addEventListener('submit', (event) => {
    event.preventDefault();
    run(event.submitter, async () => {
      preview = await call(`drafts/${encodeURIComponent(preview.id)}/review`, {
        revision: preview.revision,
        payload: preview.payload,
      });
      reviewed = true;
      applyKey = crypto.randomUUID();
      apply.disabled = false;
      result.textContent = `${preview.payload.tasks.length} tasks reviewed. Apply within ten minutes.`;
    });
  });
  apply.addEventListener('click', () => {
    if (!reviewed) return;
    run(apply, async () => {
      const value = await call('apply', {
        preview_id: preview.id,
        revision: preview.revision,
        expected_hash: preview.expected_hash,
        idempotency_key: applyKey,
      });
      reviewed = false;
      review.hidden = true;
      apply.hidden = true;
      const link = document.createElement('a');
      link.href = `/lists/${encodeURIComponent(value.list_id)}`;
      link.textContent = `${value.tasks.length} tasks saved. Open list`;
      result.replaceChildren(link);
    });
  });
}
