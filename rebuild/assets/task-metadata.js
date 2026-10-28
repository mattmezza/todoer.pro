import { request as fetch } from './request.js';
import { stowInfo, setSaveState, trackSave } from './quiet-info.js';
import { confirmAction } from './confirm.js';
// Stable entity IDs are retained separately from their display names.
export function mountTaskMetadata({
  taskId,
  accountId,
  csrf,
  container,
  canWrite,
  getRevision,
  onSaved,
  onTaskChanged,
  onConflict,
  beginWrite,
  hasUnsavedBrief,
}) {
  if (!taskId || !accountId || !container)
    throw new Error('Metadata requires a task, account and container');
  const key = `todoer:${encodeURIComponent(accountId)}:metadata:${encodeURIComponent(taskId)}`;
  let active = true,
    busy = false,
    loading = false,
    loaded = false;
  let baseline = null;
  let revision = null,
    task = null,
    selected = [],
    draft = read();
  let sequence = 0,
    suggestionSequence = 0,
    timer,
    parentTimer,
    parentSequence = 0;
  const requests = new Set();
  const root = node('section', { class: 'checklist', 'data-task-metadata': '' });
  const details = node('details');
  const notice = node('p', {
    role: 'status',
    'aria-live': 'polite',
    'data-metadata-notice': '',
  });
  const preview = node('div', { class: 'markdown', 'data-description-preview': '' });
  const chips = node('div', { class: 'tag-pills', 'data-tag-chips': '' });
  const tags = field('text', 'new_tags', {
    placeholder: 'Add tags…',
    maxlength: '5000',
  });
  const tagSuggestions = node('div', { 'data-tag-suggestions': '' });
  const section = node('select', { name: 'section_id' });
  const parent = field('text', 'parent_id', {
    placeholder: 'Exact task ID (advanced)',
    autocomplete: 'off',
  });
  const parentSearch = field('search', 'parent_search', {
    placeholder: 'Search task titles in this list',
    autocomplete: 'off',
  });
  const parentChoices = node('div', {
    class: 'grid gap-3',
    'data-parent-choices': '',
    'aria-live': 'polite',
  });
  const parentSelection = node('p', { class: 'muted', 'data-parent-selection': '' });
  const topLevel = button('Top-level task');
  const findParent = button('Find parent tasks');
  const advancedParent = node('details');
  advancedParent.append(
    node('summary', { class: 'nav-link' }, 'Advanced: exact task ID'),
    label('Parent task ID', parent),
  );
  const tagsForm = node('form', { class: 'grid gap-3', 'data-tags-form': '' });
  const sectionForm = node('form', { class: 'grid gap-3 mt-8', 'data-section-form': '' });
  const parentForm = node('form', { class: 'grid gap-3 mt-8', 'data-parent-form': '' });
  const saveTags = button('Save tags', 'submit');
  const saveSection = button('Save section', 'submit');
  const saveParent = button('Change parent', 'submit');
  const reload = button('Reset unsaved changes');
  reload.dataset.metadataReset = '';
  const unresolved = node('div', {
    class: 'grid gap-3',
    'data-unresolved-references': '',
  });
  const backlinks = node('div', { 'data-backlinks': '' });
  const referenceForm = node('form', {
    class: 'grid gap-3 mt-8',
    'data-reference-search': '',
  });
  const referenceQuery = field('text', 'reference_query', {
    placeholder: '@space #list $section %task ^subtask +tag',
    maxlength: '100',
    autocomplete: 'off',
  });
  const referenceChoices = node('div', {
    class: 'grid gap-3',
    'data-reference-choices': '',
    'aria-live': 'polite',
  });
  const selectedReference = field('text', 'selected_reference', {
    readonly: '',
    'aria-label': 'Stable reference to copy',
  });
  selectedReference.hidden = true;
  tagsForm.append(node('h3', {}, 'Tags'), chips, tagSuggestions, saveTags);
  sectionForm.append(
    label('Section', section),
    node(
      'p',
      { class: 'muted', 'data-section-note': '' },
      'Sections apply to an entire task subtree. Child tasks inherit their root section.',
    ),
    saveSection,
  );
  parentForm.append(
    label('Find a parent task', parentSearch),
    findParent,
    topLevel,
    parentSelection,
    parentChoices,
    node(
      'p',
      { class: 'muted' },
      'Choose a task in this list, or make this a top-level task. The server checks cycles and current access when saving.',
    ),
    advancedParent,
    saveParent,
  );
  topLevel.addEventListener('click', () => chooseParent('', 'Top-level task'));
  findParent.addEventListener('click', findParents);
  parentSearch.addEventListener('keydown', (event) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      findParents();
    }
  });
  parentSearch.addEventListener('input', () => {
    clearTimeout(parentTimer);
    parentSequence++;
    parentTimer = setTimeout(findParents, 250);
  });
  parent.addEventListener('input', () => {
    parentSelection.textContent = parent.value
      ? 'Advanced parent selection (not yet verified)'
      : 'Top-level task';
  });
  referenceForm.append(
    node('h3', {}, 'Find a reference'),
    label('Reference lookup', referenceQuery),
    button('Find references', 'submit', false),
    referenceChoices,
    selectedReference,
    node(
      'p',
      { class: 'muted' },
      'Select a result to copy its stable reference into the description. Save the description before resolving ambiguous names below.',
    ),
  );
  const referenceDetails = node('details', {
    class: 'mt-8',
    'data-reference-details': '',
  });
  referenceDetails.append(
    node('summary', { class: 'nav-link' }, 'References and backlinks'),
    referenceForm,
    node('h3', {}, 'Unresolved references in saved description'),
    unresolved,
    node('h3', {}, 'Backlinks'),
    backlinks,
  );
  details.append(
    node('summary', { class: 'nav-link' }, 'Tags, structure and references'),
    notice,
    node('h3', { 'data-saved-description-heading': '' }, 'Saved description'),
    preview,
    tagsForm,
    parentForm,
    reload,
    referenceDetails,
  );
  root.append(details);
  container.replaceChildren(root);
  if (draft) {
    revision = draft.revision;
    selected = draft.selected || [];
    tags.value = draft.newTags || '';
    parent.value = draft.parentId || '';
    details.open = true;
  }
  for (const form of [tagsForm, sectionForm, parentForm]) {
    form.addEventListener('input', changed);
    form.addEventListener('change', changed);
  }
  tagsForm.addEventListener('submit', async (e) => {
    e.preventDefault();
    const additions = tags.value
      .trim()
      .split(/\s+/)
      .filter(Boolean)
      .map((n) => n.replace(/^\+/, ''))
      .filter(Boolean);
    if (selected.length + additions.length > 50) {
      say('At most fifty tags are supported.');
      return;
    }
    changed();
    await mutate(
      `/tasks/${encodeURIComponent(taskId)}/tags`,
      'PUT',
      { tags: [...selected.map((t) => ({ id: t.id })), ...additions] },
      async () => {
        const order = [...selected.map((tag) => tag.id), ...additions];
        const saved = await request(`/tasks/${encodeURIComponent(taskId)}/tags`);
        const position = (tag) => {
          const index = order.findIndex(
            (value) => value === tag.id || value.toLowerCase() === tag.name.toLowerCase(),
          );
          return index < 0 ? order.length : index;
        };
        selected = saved.sort((a, b) => position(a) - position(b));
        tags.value = '';
        renderTags();
      },
      'Tags saved.',
    );
    if (active) tags.focus();
  });
  tags.addEventListener('keydown', (event) => {
    if (
      event.isComposing ||
      !['Enter', ',', ' '].includes(event.key) ||
      !tags.value.trim()
    )
      return;
    event.preventDefault();
    event.stopPropagation();
    if (!busy) tagsForm.requestSubmit();
  });
  tags.addEventListener('blur', () => {
    if (active && !busy && tags.value.trim()) tagsForm.requestSubmit();
  });
  sectionForm.addEventListener('submit', (e) => {
    e.preventDefault();
    changed();
    mutate(
      `/tasks/${encodeURIComponent(taskId)}/section`,
      'PUT',
      { section_id: section.value || null },
      null,
      'Section saved.',
    );
  });
  parentForm.addEventListener('submit', (e) => {
    e.preventDefault();
    changed();
    mutate(
      `/tasks/${encodeURIComponent(taskId)}/reparent`,
      'POST',
      { parent_id: parent.value.trim() || null },
      null,
      'Parent updated.',
    );
  });
  reload.addEventListener('click', async () => {
    if (
      busy ||
      (draft &&
        !(await confirmAction(
          'Discard the metadata draft and load saved tags, section and parent?',
        )))
    )
      return;
    draft = null;
    remove();
    load();
  });
  referenceForm.addEventListener('submit', (e) => {
    e.preventDefault();
    clearTimeout(timer);
    findReferences();
  });
  referenceQuery.addEventListener('input', () => {
    clearTimeout(timer);
    timer = setTimeout(findReferences, 250);
  });
  tags.addEventListener('input', () => {
    clearTimeout(timer);
    timer = setTimeout(findTags, 250);
  });
  renderTags();
  disabled(true);
  load();

  function node(tag, attrs = {}, text) {
    const n = document.createElement(tag);
    for (const [k, v] of Object.entries(attrs)) n.setAttribute(k, v);
    if (text !== undefined) n.textContent = text;
    return n;
  }
  function field(type, name, attrs = {}) {
    return node('input', { type, name, class: 'min-h-11', ...attrs });
  }
  function button(text, type = 'button', primary = type === 'submit') {
    return node(
      'button',
      {
        type,
        class: primary ? 'primary' : 'nav-link min-h-11 text-left',
      },
      text,
    );
  }
  function label(text, input) {
    const n = node('label');
    n.append(document.createTextNode(text), input);
    return n;
  }
  function say(message) {
    if (active)
      notice.textContent = stowInfo(message, `metadata:${taskId}`) ? '' : message;
  }
  function read() {
    try {
      const d = JSON.parse(sessionStorage.getItem(key) || 'null');
      return d && Number.isInteger(d.revision) && Array.isArray(d.selected) ? d : null;
    } catch {
      return null;
    }
  }
  function remove() {
    if (active)
      try {
        sessionStorage.removeItem(key);
      } catch {}
  }
  function changed() {
    if (!active || busy || !canWrite || !Number.isInteger(revision)) return;
    setSaveState(
      'metadata:' + taskId,
      'unsaved',
      'Metadata changes are kept on this device until you save them.',
    );
    draft = {
      revision,
      selected,
      newTags: tags.value,
      sectionId: section.value || null,
      parentId: parent.value,
      parentLabel: parentSelection.textContent,
    };
    try {
      sessionStorage.setItem(key, JSON.stringify(draft));
    } catch {
      say('Browser draft storage is unavailable. Copy metadata changes before closing.');
    }
  }
  function disabled(value) {
    for (const control of root.querySelectorAll('input,select,button')) {
      const readControl =
        control === reload ||
        control === referenceQuery ||
        control.closest('[data-reference-search]');
      control.disabled = (value && !readControl) || (!canWrite && !readControl);
    }
    for (const control of [saveTags, saveSection, saveParent]) control.hidden = !canWrite;
    saveSection.hidden = !canWrite || Boolean(task?.parent_id);
    section.disabled = value || !canWrite || Boolean(task?.parent_id);
  }
  function request(path, method = 'GET', ...args) {
    const action = () => performRequest(path, method, ...args);
    return method === 'GET' || /preview|intents/.test(path)
      ? action()
      : trackSave('metadata:' + taskId, action);
  }
  async function performRequest(path, method = 'GET', body, html = false) {
    const controller = new AbortController();
    requests.add(controller);
    const timeout = setTimeout(() => controller.abort(), 30000);
    try {
      const response = await fetch(html ? path : `/api/v1${path}`, {
        method,
        cache: 'no-store',
        credentials: 'same-origin',
        signal: controller.signal,
        headers: {
          'x-csrf-token': csrf,
          ...(body === undefined ? {} : { 'Content-Type': 'application/json' }),
        },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
      if (!response.ok) {
        const data = await response.json().catch(() => null);
        const error = new Error(
          data?.error?.message || `Metadata request failed (${response.status})`,
        );
        error.status = response.status;
        throw error;
      }
      return html ? response.text() : response.json();
    } finally {
      clearTimeout(timeout);
      requests.delete(controller);
    }
  }
  function renderTags() {
    chips.replaceChildren();
    for (const tag of selected) {
      const row = node('div', { class: 'tag-pill' });
      row.append(node('span', { 'data-tag-id': tag.id }, `+${tag.name}`));
      if (canWrite) {
        const remove = button('×');
        remove.className = 'tag-pill-remove';
        remove.setAttribute('aria-label', `Remove +${tag.name}`);
        remove.title = `Remove +${tag.name}`;
        remove.addEventListener('click', () => {
          if (busy) return;
          selected = selected.filter((t) => t.id !== tag.id);
          renderTags();
          changed();
          tagsForm.requestSubmit();
        });
        row.append(remove);
      }
      chips.append(row);
    }
    tags.classList.add('tag-inline-input');
    tags.setAttribute('aria-label', 'Add tags, separated by spaces');
    tags.placeholder = '+ Add tag';
    chips.append(tags);
    chips.hidden = false;
  }
  async function load() {
    if (!active || busy || loading) return;
    loading = true;
    const version = ++sequence;
    try {
      const tasks = await request(`/tasks/${encodeURIComponent(taskId)}/subtree`);
      const current = tasks.find((t) => t.id === taskId);
      if (!current) throw new Error('Task unavailable');
      const [savedTags, location, sections, references, links] = await Promise.all([
        request(`/tasks/${encodeURIComponent(taskId)}/tags`),
        request(`/tasks/${encodeURIComponent(taskId)}/section`),
        request(`/lists/${encodeURIComponent(current.list_id)}/sections`),
        request(`/references/task/${encodeURIComponent(taskId)}/unresolved`),
        request(`/references/task/${encodeURIComponent(taskId)}/backlinks`),
      ]);
      if (!active || version !== sequence) return;
      task = current;
      loaded = true;
      baseline = {
        tagIDs: savedTags.map((t) => t.id).sort(),
        sectionId: location.section_id || '',
        parentId: current.parent_id || '',
      };
      section.replaceChildren(node('option', { value: '' }, 'No section'));
      for (const item of sections)
        section.append(node('option', { value: item.id }, item.title));
      if (!draft) {
        revision = current.revision;
        selected = savedTags;
        tags.value = '';
        section.value = location.section_id || '';
        parent.value = current.parent_id || '';
      } else {
        selected = draft.selected.map(
          (tag) => savedTags.find((t) => t.id === tag.id) || tag,
        );
        section.value = draft.sectionId || '';
        say(
          'Metadata draft restored with its original revision. Live changes will not replace it.',
        );
      }
      await showParent();
      if (!active || version !== sequence) return;
      renderTags();
      renderUnresolved(references);
      renderBacklinks(links);
      disabled(false);
      if (!draft) say('');
      loadPreview(version);
    } catch (e) {
      if (active)
        say(`Could not load metadata: ${e.message}. Existing drafts are preserved.`);
    } finally {
      loading = false;
    }
  }
  async function loadPreview(version = sequence) {
    try {
      const html = await request(
        `/fragments/tasks/${encodeURIComponent(taskId)}/description`,
        'GET',
        undefined,
        true,
      );
      // This is the server's authenticated, sanitized saved-description fragment.
      if (active && version === sequence) preview.innerHTML = html;
    } catch {
      if (active)
        preview.textContent =
          task?.description || 'Saved description preview unavailable.';
    }
  }
  async function mutate(path, method, body, after, message, binding = false) {
    if (!active || busy || !canWrite || !loaded) return;
    const expected = revision;
    if (binding && (typeof hasUnsavedBrief !== 'function' || hasUnsavedBrief())) {
      say('Save or discard the task brief draft before binding a saved reference.');
      return;
    }
    if (getRevision?.() !== expected) {
      say(
        'The task changed. Your metadata draft is preserved; review saved data before replacing it.',
      );
      onConflict?.(new Error('Task revision changed'));
      return;
    }
    busy = true;
    disabled(true);
    let release;
    try {
      release = beginWrite?.();
      const result = await request(path, method, { revision: expected, ...body });
      if (!active) return;
      const next = result.revision ?? result.source_revision;
      if (onSaved?.(expected, next) === false) {
        say('The change was saved, but the open task changed. Your draft is preserved.');
        onConflict?.(new Error('Open task changed'));
        return;
      }
      revision = next;
      if (result.id === taskId) {
        task = result;
        onTaskChanged?.(result);
      }
      if (after) await after();
      if (!active) return;
      if (binding) {
        renderUnresolved(result.unresolved);
        loadPreview();
      }
      if (path.endsWith('/tags')) baseline.tagIDs = selected.map((t) => t.id).sort();
      if (path.endsWith('/section')) baseline.sectionId = body.section_id || '';
      if (path.endsWith('/reparent')) {
        baseline.parentId = body.parent_id || '';
        const location = await request(`/tasks/${encodeURIComponent(taskId)}/section`);
        if (!active) return;
        // An explicitly edited section stays a draft; otherwise show inherited location.
        if (section.value === baseline.sectionId)
          section.value = location.section_id || '';
        baseline.sectionId = location.section_id || '';
      }
      if (
        tags.value.trim() ||
        JSON.stringify(selected.map((t) => t.id).sort()) !==
          JSON.stringify(baseline.tagIDs) ||
        section.value !== baseline.sectionId ||
        parent.value.trim() !== baseline.parentId
      ) {
        draft = {
          revision,
          selected,
          newTags: tags.value,
          sectionId: section.value || null,
          parentId: parent.value,
          parentLabel: parentSelection.textContent,
        };
        try {
          sessionStorage.setItem(key, JSON.stringify(draft));
        } catch {}
      } else {
        draft = null;
        remove();
      }
      say(draft ? 'Other metadata edits remain unsaved.' : '');
    } catch (e) {
      if (active) {
        say(
          `Change not confirmed: ${e.message}. Your draft is preserved. Review saved data before retrying an ambiguous result.`,
        );
        if (e.status === 409) onConflict?.(e);
      }
    } finally {
      release?.();
      busy = false;
      if (active) disabled(false);
    }
  }
  function renderUnresolved(items) {
    unresolved.replaceChildren();
    if (!items.length)
      unresolved.append(node('p', { class: 'muted' }, 'No unresolved references.'));
    for (const item of items) {
      const row = node('section');
      row.append(node('p', {}, `${item.token} · occurrence ${item.occurrence + 1}`));
      if (!item.choices.length)
        row.append(
          node(
            'p',
            { class: 'muted' },
            'No currently accessible matching target. Edit and save the description to change this token.',
          ),
        );
      for (const choice of item.choices) {
        const bind = button(`Bind ${choice.label} (${choice.context || choice.kind})`);
        bind.hidden = !canWrite;
        bind.addEventListener('click', () =>
          mutate(
            `/references/task/${encodeURIComponent(taskId)}/bind`,
            'POST',
            {
              occurrence: item.occurrence,
              target_kind: choice.kind,
              target_id: choice.id,
            },
            null,
            'Reference bound to the chosen stable target.',
            true,
          ),
        );
        row.append(bind);
      }
      unresolved.append(row);
    }
  }
  function renderBacklinks(items) {
    backlinks.replaceChildren();
    if (!items.length)
      backlinks.append(
        node('p', { class: 'muted' }, 'No currently accessible backlinks.'),
      );
    for (const item of items) {
      if (item.source_kind === 'task')
        backlinks.append(
          node(
            'a',
            { href: `/tasks/${encodeURIComponent(item.source_id)}`, class: 'nav-link' },
            `Task ${item.source_id}`,
          ),
        );
      else if (item.source_task_id)
        backlinks.append(
          node(
            'a',
            {
              href: `/tasks/${encodeURIComponent(item.source_task_id)}`,
              class: 'nav-link',
            },
            `Comment ${item.source_id}`,
          ),
        );
      else backlinks.append(node('p', {}, `Comment ${item.source_id}`));
    }
  }
  function chooseParent(id, title) {
    if (busy || !canWrite) return;
    parent.value = id;
    parentSelection.textContent = title;
    parentChoices.replaceChildren();
    parentSearch.value = '';
    parentSequence++;
    changed();
  }
  async function showParent() {
    if (!parent.value) {
      parentSelection.textContent = 'Top-level task';
      return;
    }
    if (draft?.parentLabel) {
      parentSelection.textContent = draft.parentLabel;
      return;
    }
    const id = parent.value;
    parentSelection.textContent = 'Loading selected parent…';
    try {
      const items = await request(
        `/references/autocomplete?query=${encodeURIComponent(id)}&limit=1`,
      );
      if (!active || parent.value !== id) return;
      parentSelection.textContent =
        items.find((t) => t.id === id)?.label || 'Selected parent unavailable';
    } catch {
      if (active && parent.value === id)
        parentSelection.textContent =
          'Selected parent unavailable. Choose another parent or top-level.';
    }
  }
  async function findParents() {
    if (!active || !task || busy || !canWrite) return;
    clearTimeout(parentTimer);
    const version = ++parentSequence;
    try {
      const result = await request('/query', 'POST', {
        text: parentSearch.value.trim(),
        list_id: task.list_id,
        entities: ['task'],
        status: 'all',
        limit: 20,
        fields: ['title', 'context'],
      });
      if (!active || version !== parentSequence) return;
      parentChoices.replaceChildren();
      for (const hit of result.hits.filter((t) => t.id !== taskId)) {
        const context = (hit.context || [])
          .filter((c) => c.id !== hit.id)
          .map((c) => c.title)
          .join(' / ');
        const choose = button(
          `${hit.title || 'Untitled task'}${context ? ' · ' + context : ''}`,
        );
        choose.dataset.parentChoice = hit.id;
        choose.addEventListener('click', () =>
          chooseParent(hit.id, hit.title || 'Untitled task'),
        );
        parentChoices.append(choose);
      }
      if (!parentChoices.children.length)
        parentChoices.append(
          node('p', { class: 'muted' }, 'No matching parent tasks in this list.'),
        );
      if (result.next_cursor)
        parentChoices.append(
          node(
            'p',
            { class: 'muted' },
            'Showing the first matches. Refine the title search to find another task.',
          ),
        );
    } catch (e) {
      if (active && version === parentSequence)
        say(`Parent search unavailable: ${e.message}`);
    }
  }
  async function findTags() {
    if (!active || !task || busy || !canWrite) return;
    const value = tags.value.trim().split(/\s+/).at(-1)?.replace(/^\+/, '') || '';
    const version = ++suggestionSequence;
    if (!value) {
      tagSuggestions.replaceChildren();
      return;
    }
    try {
      const items = await request(
        `/lists/${encodeURIComponent(task.list_id)}/tags?prefix=${encodeURIComponent(value)}&limit=10`,
      );
      if (!active || version !== suggestionSequence) return;
      tagSuggestions.replaceChildren();
      for (const tag of items.filter((t) => !selected.some((s) => s.id === t.id))) {
        const choose = button(`Use existing +${tag.name}`);
        choose.addEventListener('click', () => {
          if (busy) return;
          selected.push(tag);
          tags.value = tags.value.trim().split(/\s+/).slice(0, -1).join(' ');
          renderTags();
          tagSuggestions.replaceChildren();
          changed();
          tagsForm.requestSubmit();
        });
        tagSuggestions.append(choose);
      }
    } catch (e) {
      if (active) say(`Tag suggestions unavailable: ${e.message}`);
    }
  }
  async function findReferences() {
    if (!active) return;
    const query = referenceQuery.value.trim();
    const version = ++suggestionSequence;
    if (!query) {
      referenceChoices.replaceChildren();
      return;
    }
    try {
      const items = await request(
        `/references/autocomplete?query=${encodeURIComponent(query)}&limit=20`,
      );
      if (!active || version !== suggestionSequence) return;
      referenceChoices.replaceChildren();
      for (const choice of items) {
        const choose = button(`${choice.label} · ${choice.context || choice.kind}`);
        choose.addEventListener('click', () => {
          const prefix = choice.prefix;
          if (!prefix) return;
          selectedReference.value = prefix + choice.id;
          selectedReference.hidden = false;
          selectedReference.focus();
          selectedReference.select();
          say(
            'Stable reference selected. Copy it into the description, then save the brief.',
          );
        });
        referenceChoices.append(choose);
      }
      if (!items.length)
        referenceChoices.append(node('p', {}, 'No accessible reference matches.'));
    } catch (e) {
      if (active) say(`Reference lookup unavailable: ${e.message}`);
    }
  }
  const cleanup = () => {
    active = false;
    sequence++;
    clearTimeout(timer);
    clearTimeout(parentTimer);
    parentSequence++;
    for (const c of requests) c.abort();
    requests.clear();
  };
  cleanup.refresh = () => {
    if (!active || busy) return;
    if (draft) {
      say('Task data changed. Your metadata draft and original revision are preserved.');
      return;
    }
    load();
  };
  return cleanup;
}
