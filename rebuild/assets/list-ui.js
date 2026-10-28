import { beginLoading } from './loading.js';
import { editorAnchor, alignEditorField } from './seamless-editor.js';
import { toast } from './toast.js';
import { confirmAction } from './confirm.js';
import { openListSettings, setupTaskSettingsDock } from './list-settings.js';
import { previewTaskMove } from './task-page.js';
export function mountListUi({ api, notice, listId }) {
  setupTaskSettingsDock(document.querySelector('main'), 'list', listId);
  const canWrite = document.body.dataset.canWrite === 'true';
  const edit = document.querySelector('[data-list-edit]');
  const sectionCreate = document.querySelector('[data-section-create]');
  const titleInput = edit?.querySelector('input[name="title"]');
  if (titleInput) {
    const titleArea = document.createElement('textarea');
    for (const attribute of titleInput.attributes) if (attribute.name !== 'value') titleArea.setAttribute(attribute.name, attribute.value);
    titleArea.value = titleInput.value;
    titleArea.rows = 1;
    titleInput.replaceWith(titleArea);
  }

  if (edit && !edit.querySelector('[name="color"]')) {
    const heading = document.createElement('div');
    heading.className = 'list-edit-heading';
    const picker = document.createElement('input');
    picker.type = 'color';
    picker.name = 'color';
    picker.className = 'list-edit-color';
    picker.setAttribute('aria-label', 'List color');
    const swatch = document.querySelector('[data-list-display] .entity-color');
    const rgb = swatch && getComputedStyle(swatch).backgroundColor.match(/\d+/g);
    picker.value =
      document.body.dataset.listColor ||
      (rgb?.length >= 3
        ? '#' +
          rgb
            .slice(0, 3)
            .map((part) => Number(part).toString(16).padStart(2, '0'))
            .join('')
        : '#3b82f6');
    const title = edit.querySelector('[name="title"]');
    title.before(heading);
    heading.append(picker, title);
  }
  const completedCheck = document.querySelector('#show-completed');
  function updateCompletedLabel() {
    for (const button of document.querySelectorAll('[data-toggle-completed]')) {
      const icon = button.querySelector('svg');
      button.replaceChildren();
      if (icon) button.append(icon);
      button.append(
        document.createTextNode(
          completedCheck.checked ? 'Hide completed' : 'Show completed',
        ),
      );
      button.setAttribute('aria-pressed', String(completedCheck.checked));
    }
  }
  completedCheck?.addEventListener('change', updateCompletedLabel);
  if (completedCheck) updateCompletedLabel();
  const closeMenus = () =>
    document
      .querySelectorAll('.entity-menu[open],.task-actions[open]')
      .forEach((menu) => {
        menu.open = false;
      });
  async function save(form, path, body) {
    const button = form.querySelector('[type="submit"]');
    if (button?.disabled) return;
    if (button) button.disabled = true;
    try {
      await api(path, 'PATCH', body);
      location.reload();
    } catch (error) {
      let output = form.querySelector('[role="alert"]');
      if (!output) {
        output = document.createElement('p');
        output.setAttribute('role', 'alert');
        form.append(output);
      }
      output.textContent = `${error.message}. Your draft is unchanged.`;
    } finally {
      if (button) button.disabled = false;
    }
  }
  document.addEventListener('click', async (event) => {
    if (event.target.closest('[data-list-edit-open]') && canWrite) {
      const titleAnchor = editorAnchor(document.querySelector('[data-list-display] h1 button'));
      const descriptionAnchor = editorAnchor(document.querySelector('.list-description'));
      closeMenus();
      edit.hidden = false;
      document.querySelector('[data-list-display]').hidden = true;
      document.querySelector('.list-description').hidden = true;
      alignEditorField(edit.querySelector('[name="title"]'), titleAnchor);
      alignEditorField(edit.querySelector('[name="description"]'), descriptionAnchor);
      edit.querySelector(event.target.closest('.list-description') ? '[name="description"]' : '[name="title"]').focus();
      edit.querySelector('[name="title"]').select();
    }
    if (event.target.closest('[data-list-edit-cancel]')) {
      edit.hidden = true;
      document.querySelector('[data-list-display]').hidden = false;
      document.querySelector('.list-description').hidden = false;
    }
    if (event.target.closest('[data-section-add-open]') && canWrite) {
      sectionCreate.hidden = false;
      document.querySelector('[data-section-add-open]').hidden = true;
      sectionCreate.querySelector('input').focus();
    }
    if (event.target.closest('[data-section-add-cancel]')) {
      sectionCreate.hidden = true;
      document.querySelector('[data-section-add-open]').hidden = false;
    }
    if (event.target.closest('[data-toggle-completed]')) {
      const check = document.querySelector('#show-completed');
      check.checked = !check.checked;
      check.dispatchEvent(new Event('change'));
      closeMenus();
    }
    if (event.target.closest('[data-list-settings-open],[data-list-sharing-open]')) {
      closeMenus();
      openListSettings(
        listId,
        event.target.closest('[data-list-sharing-open]') ? 'sharing' : 'details',
      );
    }
    const ref = event.target.closest('[data-copy-reference]');
    if (ref) {
      try {
        await navigator.clipboard.writeText(ref.dataset.copyReference);
        toast('Reference copied.');
      } catch {
        notice('Could not copy reference.', true);
      }
    }
    if (event.target.closest('[data-copy-link]')) {
      try {
        await navigator.clipboard.writeText(location.origin + `/lists/${listId}`);
        toast('Permalink copied.');
      } catch {
        notice('Copy this page address from your browser.', true);
      }
      closeMenus();
    }
    if (
      event.target.closest('[data-list-archive]') &&
      canWrite &&
      (await confirmAction('Archive this list? Its tasks and history are preserved.'))
    ) {
      try {
        await api(`/lists/${listId}`, 'PATCH', {
          revision: Number(edit.dataset.revision),
          archived: true,
        });
        location.assign('/workspaces');
      } catch (error) {
        notice(error.message, true);
      }
    }
    const sectionButton = event.target.closest('[data-section-edit-open]');
    if (sectionButton && canWrite) {
      closeMenus();
      const group = sectionButton.closest('[data-section-id]');
      try {
        const sections = await api(`/lists/${listId}/sections`);
        const section = sections.find((value) => value.id === group.dataset.sectionId);
        if (!section) throw Error('Section no longer exists');
        editSection(section);
      } catch (error) {
        notice(error.message, true);
      }
    }
    const move = event.target.closest(
      '[data-list-move-open],[data-section-move-open],[data-task-move-open]',
    );
    if (move && canWrite) {
      closeMenus();
      try {
        await openMove(move);
      } catch (error) {
        notice(error.message, true);
      }
    }
  });
  edit?.addEventListener('submit', (event) => {
    event.preventDefault();
    const data = new FormData(edit);
    save(edit, `/lists/${listId}`, {
      revision: Number(edit.dataset.revision),
      title: data.get('title'),
      description: data.get('description'),
      color: data.get('color'),
    });
  });
  sectionCreate?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const button = sectionCreate.querySelector('[type="submit"]');
    button.disabled = true;
    try {
      await api(`/lists/${listId}/sections`, 'POST', {
        title: new FormData(sectionCreate).get('title'),
        description: '',
      });
      location.reload();
    } catch (error) {
      sectionCreate.querySelector('[role="alert"]').textContent = error.message;
    } finally {
      button.disabled = false;
    }
  });
  function modal(title) {
    const dialog = document.createElement('dialog');
    dialog.className = 'quick-dialog';
    const heading = document.createElement('h2');
    heading.textContent = title;
    const close = document.createElement('button');
    close.type = 'button';
    close.className = 'dialog-close';
    close.setAttribute('aria-label', 'Close dialog');
    close.textContent = '×';
    close.onclick = () => dialog.close();
    dialog.append(close, heading);
    dialog.addEventListener('close', () => dialog.remove(), { once: true });
    document.body.append(dialog);
    return dialog;
  }
  function field(form, text, input) {
    const label = document.createElement('label');
    label.append(document.createTextNode(text), input);
    form.append(label);
    return input;
  }
  function editSection(section) {
    const dialog = modal('Edit section');
    const form = document.createElement('form');
    const title = document.createElement('input');
    title.name = 'title';
    title.required = true;
    title.maxLength = 500;
    title.value = section.title;
    field(form, 'Section name', title);
    const brief = document.createElement('textarea');
    brief.name = 'description';
    brief.value = section.description;
    brief.rows = 4;
    field(form, 'Description', brief);
    const actions = document.createElement('div');
    actions.className = 'inline-actions';
    const cancel = document.createElement('button');
    cancel.type = 'button';
    cancel.textContent = 'Cancel';
    cancel.onclick = () => dialog.close();
    const submit = document.createElement('button');
    submit.type = 'submit';
    submit.className = 'primary';
    submit.textContent = 'Save';
    actions.append(cancel, submit);
    form.append(actions);
    form.onsubmit = (event) => {
      event.preventDefault();
      save(form, `/sections/${section.id}`, {
        revision: section.revision,
        title: title.value,
        description: brief.value,
      });
    };
    dialog.append(form);
    dialog.showModal();
    title.select();
  }
  async function openMove(trigger) {
    const kind = trigger.hasAttribute('data-list-move-open')
      ? 'list'
      : trigger.hasAttribute('data-section-move-open')
        ? 'section'
        : 'task';
    const entityId =
      kind === 'list'
        ? listId
        : trigger.dataset[kind === 'section' ? 'sectionMoveOpen' : 'taskMoveOpen'];
    const originalRow = kind === 'task' ? trigger.closest('[data-id]') : null;
    const initialTask = originalRow
      ? {
          id: entityId,
          list_id: listId,
          parent_id: originalRow.dataset.parentId || null,
          revision: Number(originalRow.dataset.revision),
        }
      : null;
    const dialog = modal(`Move ${kind}`);
    const form = document.createElement('form');
    const select = document.createElement('select');
    select.required = true;
    field(form, kind === 'list' ? 'Destination space' : 'Destination list', select);
    const sections = document.createElement('select');
    sections.append(new Option('No section', ''));
    if (kind === 'task') field(form, 'Destination section', sections);
    const status = document.createElement('p');
    status.setAttribute('role', 'status');
    const ackLabel = document.createElement('label');
    const ack = document.createElement('input');
    ack.type = 'checkbox';
    ackLabel.append(
      ack,
      document.createTextNode(
        'I reviewed the access change and want to share this content with the additional members.',
      ),
    );
    ackLabel.hidden = true;
    const error = document.createElement('p');
    error.setAttribute('role', 'alert');
    const submit = document.createElement('button');
    submit.type = 'submit';
    submit.className = 'primary';
    submit.textContent = 'Review move';
    const cancel = document.createElement('button');
    cancel.type = 'button';
    cancel.textContent = 'Cancel';
    cancel.onclick = () => dialog.close();
    const actions = document.createElement('div');
    actions.className = 'inline-actions';
    actions.append(cancel, submit);
    form.append(status, ackLabel, error, actions);
    dialog.append(form);
    dialog.showModal();
    const skeleton = beginLoading(form, { variant: 'form' });
    let reviewed = null,
      preview = null,
      key = crypto.randomUUID();
    try {
      const choices = await api(kind === 'list' ? '/workspaces' : '/lists');
      for (const value of choices) {
        if (kind === 'section' && value.id === listId) continue;
        select.append(new Option(value.title, value.id));
      }
      if (!select.options.length) {
        submit.disabled = true;
        status.textContent = 'No accessible destination is available.';
        skeleton.finish();
        return;
      }
      if (kind === 'task') select.value = listId;
    } catch (failure) {
      error.textContent = failure.message;
      submit.disabled = true;
      skeleton.fail(failure.message, () => { dialog.close(); trigger.click(); });
      return;
    }
    async function changed() {
      reviewed = null;
      preview = null;
      ack.checked = false;
      ackLabel.hidden = true;
      submit.textContent = 'Review move';
      status.textContent = '';
      if (kind === 'task') {
        const sectionLoading = beginLoading(form, { variant: 'form' });
        sections.replaceChildren(new Option('No section', ''));
        try {
          for (const section of await api(`/lists/${select.value}/sections`))
            sections.append(new Option(section.title, section.id));
          submit.disabled = false;
        } catch (failure) {
          submit.disabled = true;
          sectionLoading.fail(failure.message, changed);
          error.textContent = failure.message;
        } finally { sectionLoading.finish(); }
      }
    }
    select.onchange = changed;
    sections.onchange = () => {
      reviewed = null;
      preview = null;
      submit.textContent = 'Review move';
    };
    await changed();
    skeleton.finish();
    select.focus();
    form.onsubmit = async (event) => {
      event.preventDefault();
      if (submit.disabled) return;
      submit.disabled = true;
      error.textContent = '';
      try {
        if (!reviewed) {
          const taskPlan =
            kind === 'task'
              ? await previewTaskMove(
                  api,
                  initialTask,
                  select.value,
                  sections.value || null,
                  true,
                )
              : null;
          if (kind === 'list')
            preview = await api(`/lists/${entityId}/workspace-move/preview`, 'POST', {
              destination_workspace_id: select.value,
            });
          else if (kind === 'section')
            preview = await api(`/sections/${entityId}/move/preview`, 'POST', {
              destination_list_id: select.value,
            });
          else
            preview =
              select.value === listId
                ? { added_members: 0 }
                : await api(
                    `/tasks/${entityId}/move-preview?destination=${encodeURIComponent(select.value)}`,
                  );
          const broadens = Boolean(
            preview.audience_broadens || preview.added_members > 0,
          );
          ackLabel.hidden = !broadens;
          status.textContent = preview.destination_public && !preview.source_public ? 'This move makes this content publicly viewable. Review and acknowledge before moving.' : broadens
            ? 'This move adds people who can access this content. Review and acknowledge before moving.'
            : 'The destination adds no new audience. Existing IDs, files and history are preserved.';
          reviewed = {
            destination: select.value,
            section: sections.value,
            plan: taskPlan,
          };
          submit.textContent = 'Move';
          return;
        }
        if (!ackLabel.hidden && !ack.checked)
          throw Error('Acknowledge the access change before moving');
        if (kind === 'list') {
          await api(`/lists/${entityId}/workspace-move`, 'POST', {
            destination_workspace_id: reviewed.destination,
            list_revision: preview.list_revision,
            source_workspace_revision: preview.source_workspace_revision,
            destination_workspace_revision: preview.destination_workspace_revision,
            policy_hash: preview.policy_hash,
            acknowledge_access_change: ack.checked,
          });
          location.reload();
        } else if (kind === 'section') {
          await api(`/sections/${entityId}/move`, 'POST', {
            destination_list_id: reviewed.destination,
            section_revision: preview.section_revision,
            source_list_revision: preview.source_list_revision,
            destination_list_revision: preview.destination_list_revision,
            policy_hash: preview.policy_hash,
            acknowledge_access_change: ack.checked,
          });
          location.assign(`/lists/${reviewed.destination}`);
        } else {
          const plan = reviewed.plan;
          await api('/bulk/apply', 'POST', {
            preview_id: plan.preview_id,
            expected_hash: plan.expected_hash,
            idempotency_key: key,
          });
          location.assign(`/lists/${reviewed.destination}`);
        }
      } catch (failure) {
        error.textContent = `${failure.message}. Nothing was moved unless a prior request succeeded; close and refresh to check.`;
        reviewed = null;
        submit.textContent = 'Review move';
      } finally {
        submit.disabled = false;
      }
    };
  }
}
