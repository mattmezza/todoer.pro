import { navigate, refreshPage } from './page-navigation.js';
import { pageScope } from './page-lifecycle.js';
import { beginLoading } from './loading.js';
import { editorAnchor, alignEditorField } from './seamless-editor.js';
import { toast } from './toast.js';
import { confirmAction } from './confirm.js';
import { openListSettings, setupTaskSettingsDock } from './list-settings.js';
import { previewTaskMove } from './task-page.js';
import { mountListColorPicker } from './list-color-picker.js';
export function mountListUi({ api, notice, listId }) {
  const scope = pageScope();

  setupTaskSettingsDock(document.querySelector('main'), 'list', listId);
  const canWrite = document.body.dataset.canWrite === 'true';
  const edit = document.querySelector('[data-list-edit]');
  const sectionCreate = document.querySelector('[data-section-create]');
  if (sectionCreate && !sectionCreate.querySelector('[name="description"]')) {
    const label = document.createElement('label');
    label.className = 'sr-only';
    label.htmlFor = 'new-section-description';
    label.textContent = 'Section description';
    const description = document.createElement('textarea');
    description.id = label.htmlFor;
    description.name = 'description';
    description.placeholder = 'Description';
    description.rows = 2;
    description.maxLength = 100000;
    sectionCreate.querySelector('.inline-actions').before(label, description);
  }
  const titleInput = edit?.querySelector('input[name="title"]');
  if (titleInput) {
    const titleArea = document.createElement('textarea');
    for (const attribute of titleInput.attributes)
      if (attribute.name !== 'value')
        titleArea.setAttribute(attribute.name, attribute.value);
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
    const palette = mountListColorPicker(picker);
    heading.append(palette.element, title);
  }
  const swatch = document.querySelector('[data-list-display] .entity-color');
  if (swatch && canWrite) {
    const rgb = getComputedStyle(swatch).backgroundColor.match(/\d+/g);
    const current = document.createElement('input');
    current.type = 'color';
    current.setAttribute('aria-label', 'Custom list color');
    current.value =
      rgb?.length >= 3
        ? '#' +
          rgb
            .slice(0, 3)
            .map((part) => Number(part).toString(16).padStart(2, '0'))
            .join('')
        : '#4e89f7';
    let savingColor = false;
    const palette = mountListColorPicker(current, {
      label: 'Change list color',
      onChange: async (color) => {
        if (savingColor) {
          palette.setColor(current.dataset.savedColor);
          return;
        }
        savingColor = true;
        try {
          const list = await api(`/lists/${listId}`);
          const updated = await api(`/lists/${listId}/color`, 'PUT', {
            revision: list.revision,
            color,
          });
          document.body.dataset.listColor = color;
          document.body.style.setProperty('--list-color', color);
          for (const dot of document.querySelectorAll(
            `.list-navigation[href="/lists/${listId}"] .list-dot`,
          )) {
            dot.style.backgroundColor = color;
            dot.dataset.entityColor = color;
          }
          if (edit) {
            edit.dataset.revision = updated.revision;
            const editColor = edit.querySelector('[name="color"]');
            if (editColor) {
              editColor.value = color;
              editColor.dispatchEvent(new Event('input'));
            }
          }
        } catch (error) {
          palette.setColor(current.dataset.savedColor);
          notice(error.message, true);
          return;
        } finally {
          savingColor = false;
        }
        current.dataset.savedColor = color;
        palette.setColor(color);
      },
    });
    current.dataset.savedColor = current.value;
    palette.element.classList.add('heading-list-color');
    swatch.replaceWith(palette.element);
  }
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
      refreshPage();
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
  scope.listen(document, 'click', async (event) => {
    if (event.target.closest('[data-list-edit-open]') && canWrite) {
      const titleAnchor = editorAnchor(
        document.querySelector('[data-list-display] h1 button'),
      );
      const descriptionAnchor = editorAnchor(document.querySelector('.list-description'));
      closeMenus();
      edit.hidden = false;
      document.querySelector('[data-list-display]').hidden = true;
      document.querySelector('.list-description').hidden = true;
      alignEditorField(edit.querySelector('[name="title"]'), titleAnchor, {
        expand: true,
      });
      alignEditorField(edit.querySelector('[name="description"]'), descriptionAnchor);
      const focused = edit.querySelector(
        event.target.closest('.list-description')
          ? '[name="description"]'
          : '[name="title"]',
      );
      focused.focus();
      if (focused.name === 'title') focused.select();
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
    if (event.target.closest('[data-list-settings-open],[data-list-sharing-open]')) {
      closeMenus();
      openListSettings(
        listId,
        event.target.closest('[data-list-sharing-open]') ? 'sharing' : 'details',
      );
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
        refreshPage();
      } catch (error) {
        notice(error.message, true);
      }
    }
    const archiveSection = event.target.closest('[data-section-archive]');
    if (archiveSection && canWrite && !archiveSection.disabled) {
      const section = archiveSection.closest('[data-section-id]');
      closeMenus();
      archiveSection.disabled = true;
      try {
        if (
          await confirmAction(
            'This section and its tasks will leave active views. Tasks, files and history are preserved. Restore it from Archived sections in the list menu.',
            { title: 'Archive this section?', acceptLabel: 'Archive section' },
          )
        ) {
          await api(`/cli/lifecycle/section/${section.dataset.sectionId}`, 'POST', {
            archive: true,
          });
          refreshPage();
        }
      } catch (error) {
        notice(error.message, true);
      } finally {
        archiveSection.disabled = false;
      }
    }
    if (event.target.closest('[data-archived-sections-open]') && canWrite) {
      closeMenus();
      await archivedSections();
    }
    const completeAll = event.target.closest(
      '[data-list-complete],[data-section-complete]',
    );
    if (completeAll && canWrite && !completeAll.disabled) {
      closeMenus();
      completeAll.disabled = true;
      try {
        await completeScope(completeAll);
      } catch (error) {
        notice(error.message, true);
      } finally {
        completeAll.disabled = false;
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
  scope.listen(edit, 'submit', (event) => {
    event.preventDefault();
    const data = new FormData(edit);
    save(edit, `/lists/${listId}`, {
      revision: Number(edit.dataset.revision),
      title: data.get('title'),
      description: data.get('description'),
      color: data.get('color'),
    });
  });
  scope.listen(sectionCreate, 'submit', async (event) => {
    event.preventDefault();
    const button = sectionCreate.querySelector('[type="submit"]');
    button.disabled = true;
    try {
      const data = new FormData(sectionCreate);
      await api(`/lists/${listId}/sections`, 'POST', {
        title: data.get('title'),
        description: data.get('description'),
      });
      refreshPage();
    } catch (error) {
      sectionCreate.querySelector('[role="alert"]').textContent = error.message;
    } finally {
      button.disabled = false;
    }
  });
  function modal(title) {
    const dialog = document.createElement('dialog');
    dialog.className = 'quick-dialog';
    dialog.setAttribute('aria-label', title);
    const heading = document.createElement('h2');
    heading.textContent = title;
    const close = document.createElement('button');
    close.type = 'button';
    close.className = 'dialog-close';
    close.setAttribute('aria-label', 'Close dialog');
    close.textContent = '×';
    close.onclick = () => dialog.close();
    dialog.append(close, heading);
    scope.listen(dialog, 'close', () => dialog.remove(), { once: true });
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
    title.dataset.referencePrefix = '$';
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
  async function completeScope(trigger) {
    const section = trigger.closest('[data-section-id]');
    const tasks = await api(`/lists/${listId}/tasks`);
    const sectionRoots = section ? new Set() : null;
    if (section) {
      let cursor = null;
      do {
        const page = await api('/query', 'POST', {
          entities: ['task'],
          list_id: listId,
          section_id: section.dataset.sectionId,
          status: 'all',
          limit: 100,
          sort: 'id',
          fields: [],
          cursor,
        });
        for (const hit of page.hits) sectionRoots.add(hit.id);
        cursor = page.next_cursor;
      } while (cursor);
    }
    const byId = new Map(tasks.map((task) => [task.id, task]));
    const unfinishedRoots = new Set();
    for (const task of tasks) {
      if (task.complete) continue;
      let root = task;
      while (root.parent_id && byId.has(root.parent_id)) root = byId.get(root.parent_id);
      unfinishedRoots.add(root.id);
    }
    const roots = tasks.filter(
      (task) =>
        !task.parent_id &&
        unfinishedRoots.has(task.id) &&
        (!sectionRoots || sectionRoots.has(task.id)),
    );
    if (!roots.length) {
      toast('There are no unfinished tasks to complete.');
      return;
    }
    if (roots.length > 50)
      throw Error(
        'This action supports up to 50 top-level tasks. Complete smaller sections separately.',
      );
    const preview = await api('/bulk/preview', 'POST', {
      operations: roots.map((task) => ({
        op: 'complete',
        id: task.id,
        revision: task.revision,
        complete_subtree: true,
      })),
    });
    const affected = preview.steps
      .flatMap((step) => step.before)
      .filter((task) => !task.complete);
    if (!affected.length) {
      toast('All tasks are already complete.');
      return;
    }
    if (
      !(await confirmAction(
        `Mark ${affected.length} unfinished task${affected.length === 1 ? '' : 's'} complete in this ${section ? 'section' : 'list'}, including sub-tasks and tasks hidden by filters? Files and history are preserved. Recurring tasks follow their completion rules.`,
        { title: 'Mark all tasks complete?', acceptLabel: 'Mark all complete' },
      ))
    )
      return;
    await api('/bulk/apply', 'POST', {
      preview_id: preview.preview_id,
      expected_hash: preview.expected_hash,
      idempotency_key: crypto.randomUUID(),
    });
    refreshPage();
  }
  async function archivedSections() {
    const dialog = modal('Archived sections');
    dialog.classList.add('archived-sections-dialog');
    const status = document.createElement('p');
    status.setAttribute('role', 'status');
    status.textContent = 'Loading archived sections…';
    dialog.append(status);
    let restored = false;
    scope.listen(dialog, 'close', () => {
      if (restored) refreshPage();
    });
    dialog.showModal();
    try {
      const sections = await api(
        `/cli/overview/sections?list=${encodeURIComponent(listId)}&archived_only=true`,
      );
      status.textContent = sections.length
        ? 'Restore a section with its archived tasks, files and history.'
        : 'No archived sections in this list.';
      for (const section of sections) {
        const row = document.createElement('div');
        row.className = 'archived-section-row';
        const copy = document.createElement('div');
        const name = document.createElement('strong');
        name.className = 'archived-section-name';
        name.textContent = `$${section.title}`;
        copy.append(name);
        if (section.description) {
          const description = document.createElement('p');
          description.textContent = section.description;
          copy.append(description);
        }
        const restore = document.createElement('button');
        restore.type = 'button';
        restore.className = 'primary';
        restore.textContent = 'Restore';
        restore.setAttribute('aria-label', `Restore section ${section.title}`);
        restore.onclick = async () => {
          restore.disabled = true;
          try {
            await api(`/cli/lifecycle/section/${section.id}`, 'POST', { archive: false });
            restored = true;
            row.remove();
            if (!dialog.querySelector('.archived-section-row'))
              status.textContent = 'All sections restored. Close to return to your list.';
          } catch (error) {
            status.textContent = error.message;
            restore.disabled = false;
          }
        };
        row.append(copy, restore);
        dialog.append(row);
      }
    } catch (error) {
      status.textContent = error.message;
    }
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
    const search = document.createElement('input');
    if (kind === 'section') {
      search.type = 'search';
      search.placeholder = 'Search lists…';
      search.autocomplete = 'off';
      field(form, 'Search destination lists', search);
    }
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
    let choices = [];
    try {
      choices = await api(kind === 'list' ? '/workspaces' : '/lists');
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
      skeleton.fail(failure.message, () => {
        dialog.close();
        trigger.click();
      });
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
        } finally {
          sectionLoading.finish();
        }
      }
    }
    select.onchange = changed;
    search.oninput = () => {
      const previous = select.value;
      const query = search.value.trim().toLocaleLowerCase();
      const matches = choices.filter(
        (value) => value.id !== listId && value.title.toLocaleLowerCase().includes(query),
      );
      select.replaceChildren(
        ...matches.map((value) => new Option(value.title, value.id)),
      );
      if (matches.some((value) => value.id === previous)) select.value = previous;
      submit.disabled = !matches.length;
      changed();
      if (!matches.length) status.textContent = 'No matching lists.';
    };
    sections.onchange = () => {
      reviewed = null;
      preview = null;
      submit.textContent = 'Review move';
    };
    await changed();
    skeleton.finish();
    (kind === 'section' ? search : select).focus();
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
          status.textContent =
            preview.destination_public && !preview.source_public
              ? 'This move makes this content publicly viewable. Review and acknowledge before moving.'
              : broadens
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
          refreshPage();
        } else if (kind === 'section') {
          await api(`/sections/${entityId}/move`, 'POST', {
            destination_list_id: reviewed.destination,
            section_revision: preview.section_revision,
            source_list_revision: preview.source_list_revision,
            destination_list_revision: preview.destination_list_revision,
            policy_hash: preview.policy_hash,
            acknowledge_access_change: ack.checked,
          });
          navigate(`/lists/${reviewed.destination}`);
        } else {
          const plan = reviewed.plan;
          await api('/bulk/apply', 'POST', {
            preview_id: plan.preview_id,
            expected_hash: plan.expected_hash,
            idempotency_key: key,
          });
          navigate(`/lists/${reviewed.destination}`);
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
