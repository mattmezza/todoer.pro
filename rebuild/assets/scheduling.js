import { preferredTimeFormatter, zonedLocalTimestamp } from './preferred-time.js';
import { stowInfo, setSaveState, trackSave } from './quiet-info.js';
import { confirmAction } from './confirm.js';
// Task scheduling is saved separately from the task brief. Every draft retains
// its original task revision until an explicit save or discard.
export function mountScheduling({
  taskId,
  accountId,
  csrf,
  container,
  canWrite,
  getRevision,
  onSaved,
  onConflict,
  beginWrite,
}) {
  if (!taskId || !accountId || !container)
    throw new Error('Scheduling requires a task, account and container');
  const prefix = `todoer:${encodeURIComponent(accountId)}:scheduling:${encodeURIComponent(taskId)}:`;
  const requests = new Set();
  let autoSaveTimer;
  let active = true;
  let loading = false;
  let busy = false;
  let loadVersion = 0;
  let baseRevision = null;
  let recurrenceRevision = null;
  let scheduleDraft = read('schedule');
  let recurrenceDraft = read('recurrence');
  let storedSchedule = null;
  let seriesCreated = false;
  let defaultZone = 'UTC';
  let formatPreferred;
  preferredTimeFormatter().then((format) => {
    if (!active) return;
    formatPreferred = format;
    defaultZone = format.timeZone;
    if (!dueLocal.value) dueZone.value = defaultZone;
    if (!start.value) recurrenceZone.value = defaultZone;
    if (storedSchedule) showPriorityPreview(storedSchedule.schedule);
  });
  const root = el('section', { class: 'checklist', 'data-scheduling': '' });
  const details = el('details');
  const summary = el('summary', { class: 'nav-link' }, 'Planning and recurrence');
  const notice = el('p', {
    role: 'status',
    'aria-live': 'polite',
    'data-schedule-notice': '',
  });
  const form = el('form', { class: 'grid gap-3', 'data-schedule-form': '' });
  const recurrence = el('form', { class: 'grid gap-3 mt-8', 'data-recurrence-form': '' });
  const planned = input('date', 'planned_date');
  const dueKind = select('due_kind', [
    ['none', 'No due date'],
    ['date', 'Due date only'],
    ['timed', 'Due date and time'],
  ]);
  const dueDate = input('date', 'due_date');
  const dueLocal = input('datetime-local', 'due_local');
  const dueZone = input('text', 'due_timezone', defaultZone);
  const priority = select('priority', [
    ['0', 'P0 · Highest'],
    ['1', 'P1 · High'],
    ['2', 'P2 · Medium'],
    ['3', 'P3 · Low'],
  ]);
  const reminders = el('div', { 'data-reminders': '' });
  const addReminder = el(
    'button',
    { type: 'button', class: 'secondary min-h-11' },
    'Add reminder',
  );
  const save = el('button', { type: 'submit', class: 'primary' }, 'Save planning');
  const reload = el(
    'button',
    { type: 'button', class: 'secondary min-h-11' },
    'Reset unsaved changes',
  );
  const rule = select('rule', [
    ['daily', 'Every N days'],
    ['weekdays', 'Selected weekdays'],
    ['completion_relative', 'Days after completion'],
  ]);
  const interval = input('number', 'interval_days', '1', {
    min: '1',
    max: '3650',
    step: '1',
  });
  const weekdays = el('fieldset', { 'data-weekdays': '' });
  weekdays.append(el('legend', {}, 'Weekdays'));
  for (const [index, name] of [
    'Monday',
    'Tuesday',
    'Wednesday',
    'Thursday',
    'Friday',
    'Saturday',
    'Sunday',
  ].entries()) {
    const box = input('checkbox', 'weekday', String(index + 1));
    weekdays.append(label(name, box));
  }
  const start = input('datetime-local', 'start_local');
  const recurrenceZone = input('text', 'recurrence_timezone', defaultZone);
  const end = input('date', 'end_date');
  const count = input('number', 'count', '', { min: '1', max: '100000', step: '1' });
  const startSeries = el(
    'button',
    { type: 'submit', class: 'primary' },
    'Start recurrence',
  );
  const discardRecurrence = el(
    'button',
    { type: 'button', class: 'secondary min-h-11' },
    'Discard recurrence draft',
  );
  const seriesLink = el(
    'a',
    { href: '/recurring', class: 'nav-link' },
    'Manage existing recurring series',
  );
  form.append(
    label('Work on', planned),
    el(
      'p',
      { class: 'muted' },
      'When you intend to work on this task. This is not a deadline.',
    ),
    label('Deadline', dueKind),
    label('Due date', dueDate),
    label('Date and time', dueLocal),
    label('Timezone', dueZone),
    label('Priority', priority),
    el('h3', {}, 'Reminders'),
    el(
      'p',
      { class: 'muted' },
      'Reminders go to the account saving this schedule. Email or push delivery requires that channel to be enabled in your preferences.',
    ),
    reminders,
    addReminder,
    save,
    reload,
  );
  recurrence.append(
    el('h3', {}, 'Start a recurring series'),
    el(
      'p',
      { class: 'muted' },
      'Available for an unfinished root task with no sub-tasks. Starting a series sets this task’s due date and time to the start below. Finish editing the planning fields first. Existing series can be paused or stopped on the Recurring page.',
    ),
    label('Repeat rule', rule),
    label('Interval in days', interval),
    weekdays,
    label('First due local date and time', start),
    label('Recurrence IANA timezone', recurrenceZone),
    label('End date (optional)', end),
    label('Total occurrences including this task (optional)', count),
    startSeries,
    discardRecurrence,
    seriesLink,
  );
  const priorityGroup = el('fieldset', { class: 'priority-selector' });
  priorityGroup.append(el('legend', {}, 'Priority'));
  const priorityOptions = el('div', { class: 'priority-options' });
  for (const [value, text] of [
    ['0', 'P0'],
    ['1', 'P1'],
    ['2', 'P2'],
    ['3', 'P3'],
  ]) {
    const choice = input('radio', 'priority_choice', value);
    choice.setAttribute('aria-label', `${text} priority`);
    const option = el('label', {
      class: 'priority-option',
      'data-priority': value || 'none',
    });
    const caption = el('span', {}, text);
    iconFor(caption, value ? 'flag' : 'minus');
    option.append(choice, caption);
    choice.addEventListener('change', () => {
      if (choice.checked) {
        priority.value = value;
        priority.dispatchEvent(new Event('change', { bubbles: true }));
      }
    });
    priorityOptions.append(option);
  }
  priorityGroup.append(priorityOptions);
  priority.closest('label').replaceWith(priorityGroup);
  priority.hidden = true;
  form.append(priority);
  const timedRow = el('div', { class: 'planning-timed-row' });
  const localLabel = dueLocal.closest('label');
  const zoneLabel = dueZone.closest('label');
  localLabel.before(timedRow);
  timedRow.append(localLabel, zoneLabel);
  const zones = el('datalist', { id: `timezones-${taskId}` });
  let zoneNames;
  try {
    zoneNames = Intl.supportedValuesOf('timeZone');
  } catch {
    zoneNames = [];
  }
  for (const name of [...new Set([defaultZone, 'UTC', ...zoneNames])].sort())
    zones.append(el('option', { value: name }));
  dueZone.setAttribute('list', zones.id);
  dueZone.setAttribute('autocomplete', 'off');
  dueZone.setAttribute('spellcheck', 'false');
  dueZone.placeholder = 'Search timezones…';
  root.append(zones);
  for (const control of [planned, dueKind, dueDate, dueLocal, dueZone]) {
    const wrapper = control.closest('label');
    const caption = el(
      'span',
      { class: 'planning-label-caption' },
      wrapper.firstChild.textContent,
    );
    wrapper.firstChild.replaceWith(caption);
    iconFor(caption, control === dueKind ? 'clock' : 'calendar');
  }
  const plannedIntent = el('details', { class: 'planning-date-intent' });
  const plannedSummary = el('summary');
  const plannedCaption = el('span', {}, 'Set planned date');
  iconFor(plannedSummary, 'calendar');
  plannedSummary.append(plannedCaption);
  const plannedLabel = planned.closest('label');
  const plannedHelp = plannedLabel.nextElementSibling;
  plannedLabel.before(plannedIntent);
  plannedIntent.append(plannedSummary, plannedLabel, plannedHelp);
  const dueIntent = el('details', { class: 'planning-date-intent' });
  const dueSummary = el('summary');
  const dueCaption = el('span', {}, 'Set due date');
  iconFor(dueSummary, 'clock');
  dueSummary.append(dueCaption);
  const dueLabel = dueKind.closest('label');
  dueLabel.before(dueIntent);
  dueIntent.append(dueSummary, dueLabel, dueDate.closest('label'), timedRow);
  // The summary is the heading; keep field labels available to screen readers.
  for (const control of [planned, dueKind, dueDate])
    control.closest('label').querySelector('.planning-label-caption').classList.add('sr-only');
  plannedHelp.remove();
  function quickDates(kind) {
    const shortcuts = el('div', { class: 'planning-date-shortcuts', role: 'group', 'aria-label': `Quick ${kind} date` });
    for (const [caption, offset] of [['+0d', 0], ['+1d', 1], ['+1w', 7], ['+2w', 14]]) {
      const button = el('button', { type: 'button', title: ['Today', 'Tomorrow', 'In one week', 'In two weeks'][[0, 1, 7, 14].indexOf(offset)] }, caption);
      button.onclick = () => {
        if (!canWrite || busy) return;
        const parts = new Intl.DateTimeFormat('en-US', { timeZone: defaultZone, year: 'numeric', month: '2-digit', day: '2-digit' }).formatToParts(new Date());
        const part = name => parts.find(p => p.type === name).value;
        const date = new Date(`${part('year')}-${part('month')}-${part('day')}T12:00:00Z`);
        date.setUTCDate(date.getUTCDate() + offset);
        const value = date.toISOString().slice(0, 10);
        if (kind === 'planned') planned.value = value;
        else { dueKind.value = 'date'; dueDate.value = value; }
        (kind === 'planned' ? planned : dueKind).dispatchEvent(new Event('change', { bubbles: true }));
        dateCaptions();
      };
      shortcuts.append(button);
    }
    return shortcuts;
  }
  plannedSummary.after(quickDates('planned'));
  dueSummary.after(quickDates('due'));
  function dateCaptions() {
    const readable = (value) => {
      if (!value) return '';
      const date = new Date(value.includes('T') ? value : `${value}T12:00:00`);
      return Number.isNaN(date.getTime())
        ? value
        : date.toLocaleDateString(undefined, {
            month: 'short',
            day: 'numeric',
            year: 'numeric',
          });
    };
    plannedCaption.textContent = planned.value
      ? `Planned · ${readable(planned.value)}`
      : 'Set planned date';
    const date =
      dueKind.value === 'date'
        ? dueDate.value
        : dueKind.value === 'timed'
          ? dueLocal.value
          : '';
    dueCaption.textContent = date ? `Due · ${readable(date)}` : 'Set due date';
  }
  const reminderGroup = el('details', { class: 'planning-group' });
  const reminderSummary = el('summary', {}, 'Reminders');
  reminderGroup.append(reminderSummary);
  const reminderHeading = [...form.children].find(
    (node) => node.tagName === 'H3' && node.textContent === 'Reminders',
  );
  for (
    let node = reminderHeading?.nextElementSibling;
    node && node !== save && node !== reload;
  ) {
    const next = node.nextElementSibling;
    reminderGroup.append(node);
    node = next;
  }
  reminderHeading?.replaceWith(reminderGroup);
  const recurrenceGroup = el('details', { class: 'planning-group' });
  recurrenceGroup.append(el('summary', {}, 'Repeat this task'), recurrence);
  recurrence.querySelector('h3')?.remove();
  const actions = el('div', { class: 'planning-actions' });
  actions.append(reload, save);
  form.append(actions);
  details.append(summary, notice, form, recurrenceGroup);
  function iconFor(element, name) {
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    const use = document.createElementNS('http://www.w3.org/2000/svg', 'use');
    svg.classList.add('ui-icon');
    svg.setAttribute('aria-hidden', 'true');
    use.setAttribute('href', `/assets/icons.svg#${name}`);
    svg.append(use);
    element.prepend(svg);
  }
  iconFor(reminderSummary, 'bell');
  iconFor(recurrenceGroup.querySelector('summary'), 'repeat');
  iconFor(save, 'check');
  iconFor(reload, 'arrow-uturn-left');
  iconFor(addReminder, 'plus');
  addReminder.classList.add('reminder-add');
  recurrence.classList.add('recurrence-compact');
  const recurrenceWhen = el('div', { class: 'reminder-when' });
  start.closest('label').before(recurrenceWhen);
  recurrenceWhen.append(start.closest('label'), recurrenceZone.closest('label'));
  const recurrenceLimits = el('div', { class: 'recurrence-limits' });
  end.closest('label').before(recurrenceLimits);
  recurrenceLimits.append(end.closest('label'), count.closest('label'));
  const recurrenceActions = el('div', { class: 'recurrence-actions' });
  recurrenceActions.append(startSeries, discardRecurrence);
  seriesLink.before(recurrenceActions);
  reload.hidden = true;
  function updateDraftActions() {
    reload.hidden = true;
  }
  form.addEventListener('input', updateDraftActions);
  form.addEventListener('change', updateDraftActions);
  recurrence.addEventListener('input', updateDraftActions);
  recurrence.addEventListener('change', updateDraftActions);
  save.hidden = true;
  stowInfo(
    'Planning and reminder changes save automatically.',
    `planning-autosave:${taskId}`,
  );
  root.append(details);
  container.replaceChildren(root);
  if (!canWrite) {
    save.hidden = true;
    addReminder.hidden = true;
    startSeries.hidden = true;
    discardRecurrence.hidden = true;
  }
  if (scheduleDraft) {
    baseRevision = scheduleDraft.revision;
    applyValues(scheduleDraft.values);
    details.open = true;
  } else applyValues({});
  if (recurrenceDraft) {
    recurrenceRevision = recurrenceDraft.revision;
    applyRecurrence(recurrenceDraft.values);
    details.open = true;
  }
  updateVisibility();
  setDisabled(true);
  form.addEventListener('input', scheduleChanged);
  form.addEventListener('change', scheduleChanged);
  recurrence.addEventListener('input', recurrenceChanged);
  recurrence.addEventListener('change', recurrenceChanged);
  form.addEventListener('submit', saveSchedule);
  recurrence.addEventListener('submit', createSeries);
  addReminder.addEventListener('click', () => {
    if (busy || !canWrite) return;
    if (reminders.children.length >= 10) {
      say('At most ten reminders are supported.');
      return;
    }
    addReminderRow({ channel: 'email', local: '', timezone: defaultZone });
    scheduleChanged();
  });
  reload.addEventListener('click', async () => {
    if (busy) return;
    if (
      (scheduleDraft || recurrenceDraft) &&
      !(await confirmAction(
        'Reset unsaved planning and recurrence changes to the last saved values?',
      ))
    )
      return;
    scheduleDraft = null;
    recurrenceDraft = null;
    remove('schedule');
    remove('recurrence');
    applyRecurrence({});
    load(true);
  });
  discardRecurrence.addEventListener('click', async () => {
    if (busy || !recurrenceDraft) return;
    if (!(await confirmAction('Discard this recurrence draft?'))) return;
    recurrenceDraft = null;
    remove('recurrence');
    recurrenceRevision = baseRevision;
    applyRecurrence({});
    updateVisibility();
    say('Recurrence draft discarded.');
  });
  load();

  function el(tag, attrs = {}, text) {
    const node = document.createElement(tag);
    for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value);
    if (text !== undefined) node.textContent = text;
    return node;
  }
  function input(type, name, value = '', attrs = {}) {
    const node = el('input', {
      type,
      name,
      ...(type === 'checkbox' ? {} : { class: 'min-h-11' }),
      ...attrs,
    });
    node.value = value;
    if (type === 'datetime-local') node.step = '1';
    return node;
  }
  function select(name, options) {
    const node = el('select', { name });
    for (const [value, text] of options) node.append(el('option', { value }, text));
    return node;
  }
  function label(text, control) {
    const node = el('label', { class: 'grid gap-3' });
    node.append(document.createTextNode(text), control);
    return node;
  }
  function read(key) {
    try {
      const value = JSON.parse(sessionStorage.getItem(prefix + key) || 'null');
      return value &&
        Number.isInteger(value.revision) &&
        value.values &&
        typeof value.values === 'object'
        ? value
        : null;
    } catch {
      return null;
    }
  }
  function persist(key, value) {
    if (!active) return;
    try {
      sessionStorage.setItem(prefix + key, JSON.stringify(value));
    } catch {
      say(
        'Browser draft storage is unavailable. Copy your planning changes before closing this panel.',
      );
    }
  }
  function remove(key) {
    if (active) {
      try {
        sessionStorage.removeItem(prefix + key);
      } catch {}
    }
  }
  function say(text, isError = false) {
    if (!active) return;
    if (isError) {
      setSaveState(`planning:${taskId}`, 'error', text);
      notice.textContent = '';
      return;
    }
    notice.textContent = stowInfo(text, `planning:${taskId}`) ? '' : text;
  }
  function setDisabled(value) {
    for (const control of root.querySelectorAll('input,select,button')) {
      control.disabled =
        (value && control !== reload) ||
        (!canWrite && control !== reload) ||
        (control === startSeries && seriesCreated);
    }
  }
  function local(value) {
    return value && value.length === 16 ? `${value}:00` : value;
  }
  function scheduleValues() {
    return {
      planned: planned.value,
      dueKind: dueKind.value,
      dueDate: dueDate.value,
      dueLocal: dueLocal.value,
      dueZone: dueZone.value,
      priority: priority.value,
      reminders: [...reminders.children].map((row) => ({
        channel: row.querySelector('[name=channel]').value,
        local: row.querySelector('[name=reminder_local]').value,
        timezone: row.querySelector('[name=reminder_timezone]').value,
      })),
    };
  }
  function recurrenceValues() {
    return {
      rule: rule.value,
      interval: interval.value,
      weekdays: [...weekdays.querySelectorAll('input:checked')].map((n) =>
        Number(n.value),
      ),
      start: start.value,
      timezone: recurrenceZone.value,
      end: end.value,
      count: count.value,
    };
  }
  function applyValues(values) {
    planned.value = values.planned || '';
    dueKind.value = values.dueKind || 'none';
    dueDate.value = values.dueDate || '';
    dueLocal.value = values.dueLocal || '';
    dueZone.value = values.dueZone || defaultZone;
    priority.value = values.priority || '';
    for (const radio of priorityOptions.querySelectorAll('input'))
      radio.checked = radio.value === priority.value;
    reminders.replaceChildren();
    for (const reminder of (values.reminders || []).slice(0, 10))
      addReminderRow(reminder);
    updateVisibility();
  }
  function applyRecurrence(values) {
    rule.value = values.rule || 'daily';
    interval.value = values.interval || '1';
    start.value = values.start || '';
    recurrenceZone.value = values.timezone || defaultZone;
    end.value = values.end || '';
    count.value = values.count || '';
    for (const box of weekdays.querySelectorAll('input'))
      box.checked = (values.weekdays || []).includes(Number(box.value));
  }
  function addReminderRow(values) {
    const row = el('fieldset', { class: 'reminder-card' });
    row.append(el('legend', { class: 'sr-only' }, 'Reminder'));
    const channel = select('channel', [
      ['email', 'Email'],
      ['push', 'Push notification'],
    ]);
    channel.value = values.channel || 'email';
    const at = input('datetime-local', 'reminder_local', values.local || '');
    at.required = true;
    const timezone = input('text', 'reminder_timezone', values.timezone || defaultZone);
    timezone.required = true;
    const remove = el(
      'button',
      { type: 'button', class: 'secondary min-h-11' },
      'Remove reminder',
    );
    remove.hidden = !canWrite;
    remove.addEventListener('click', () => {
      if (busy || !canWrite) return;
      row.remove();
      scheduleChanged();
    });
    row.append(label('Channel', channel));
    const when = el('div', { class: 'reminder-when' });
    when.append(label('Date and time', at), label('Timezone', timezone));
    row.append(when, remove);
    remove.classList.add('reminder-remove');
    reminders.append(row);
  }
  function updateVisibility() {
    dateCaptions();
    timedRow.hidden = dueKind.value !== 'timed';
    dueDate.closest('label').hidden = dueKind.value !== 'date';
    dueLocal.closest('label').hidden = dueKind.value !== 'timed';
    dueZone.closest('label').hidden = dueKind.value !== 'timed';
    dueDate.required = dueKind.value === 'date';
    dueLocal.required = dueKind.value === 'timed';
    dueZone.required = dueKind.value === 'timed';
    weekdays.hidden = rule.value !== 'weekdays';
    interval.closest('label').hidden = rule.value === 'weekdays';
  }
  function scheduleChanged() {
    if (!active || busy || !canWrite) return;
    updateVisibility();
    if (!Number.isInteger(baseRevision)) return;
    scheduleDraft = { revision: baseRevision, values: scheduleValues() };
    persist('schedule', scheduleDraft);
    setSaveState(
      'planning:' + taskId,
      'unsaved',
      'Planning changes are waiting to save automatically. Your draft is kept on this device.',
    );
    if (storedSchedule) showPriorityPreview(storedSchedule.schedule);
    queueAutoSave();
  }
  function queueAutoSave() {
    clearTimeout(autoSaveTimer);
    if (!active || busy || !canWrite || !scheduleDraft || !form.checkValidity()) return;
    autoSaveTimer = setTimeout(() => saveSchedule(), 450);
  }
  window.addEventListener('online', queueAutoSave);
  function recurrenceChanged() {
    if (!active || busy || !canWrite) return;
    updateVisibility();
    if (!Number.isInteger(recurrenceRevision)) return;
    recurrenceDraft = { revision: recurrenceRevision, values: recurrenceValues() };
    persist('recurrence', recurrenceDraft);
    setSaveState(
      'planning:' + taskId,
      'unsaved',
      'Your recurring-task changes are kept on this device until you create the series.',
    );
  }
  function request(path, method = 'GET', ...args) {
    const action = () => performRequest(path, method, ...args);
    return method === 'GET' || /preview|intents/.test(path)
      ? action()
      : trackSave('planning:' + taskId, action);
  }
  async function performRequest(path, method = 'GET', body) {
    const controller = new AbortController();
    requests.add(controller);
    const timeout = setTimeout(() => controller.abort(), 30000);
    try {
      const r = await fetch(`/api/v1${path}`, {
        method,
        credentials: 'same-origin',
        cache: 'no-store',
        signal: controller.signal,
        headers: {
          'x-csrf-token': csrf,
          ...(body === undefined ? {} : { 'Content-Type': 'application/json' }),
        },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
      if (!r.ok) {
        const data = await r.json().catch(() => null);
        const error = new Error(
          data?.error?.message || `Scheduling request failed (${r.status})`,
        );
        error.status = r.status;
        throw error;
      }
      return await r.json();
    } finally {
      clearTimeout(timeout);
      requests.delete(controller);
    }
  }
  function fromSchedule(schedule) {
    return {
      planned: schedule.planned?.date || '',
      dueKind: schedule.due?.kind || 'none',
      dueDate: schedule.due?.date || '',
      dueLocal: schedule.due?.local || '',
      dueZone: schedule.due?.timezone || defaultZone,
      priority: String(schedule.priority ?? 2),
      reminders: (schedule.reminders || []).map((r) => ({
        channel: r.channel,
        local: r.at.local,
        timezone: r.at.timezone,
      })),
    };
  }
  function showPriorityPreview(schedule) {
    const priority = scheduleDraft
      ? Number(scheduleDraft.values.priority)
      : schedule.priority;
    showDue({ ...schedule, priority }, priority !== schedule.priority);
  }
  function showDue(schedule, unsaved = false) {
    const page = document.querySelector('[data-task-page]');
    if (!page || page.dataset.taskId !== taskId) return;
    let summary = page.querySelector('[data-task-due]');
    if (!summary) {
      summary = document.createElement('button');
      summary.type = 'button';
      summary.dataset.taskDue = '';
      summary.className = 'task-due-summary';
      summary.addEventListener('click', async () => {
        const { openTaskSettings } = await import('./list-settings.js');
        openTaskSettings(page, 'planning');
      });
      page.querySelector('.task-page-heading').after(summary);
    }
    page.dataset.priority = String(schedule?.priority ?? 2);
    summary.replaceChildren();
    summary.hidden = false;
    summary.title = 'Edit planning';
    {
      const priorityBadge = document.createElement('span');
      priorityBadge.className = 'task-priority-summary';
      priorityBadge.dataset.priority = String(schedule?.priority ?? 2);
      const names = {
        0: 'P0 · Highest priority',
        1: 'P1 · High priority',
        2: 'P2 · Medium priority',
        3: 'P3 · Low priority',
      };
      priorityBadge.textContent = names[schedule?.priority ?? 2];
      iconFor(priorityBadge, 'flag');
      const line = document.createElement('span');
      line.className = 'task-priority-line';
      line.append(priorityBadge);
      summary.append(line);
    }
    const due = schedule?.due || schedule?.planned;
    if (!due) return;
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.classList.add('ui-icon');
    svg.setAttribute('viewBox', '0 0 24 24');
    svg.setAttribute('fill', 'none');
    svg.setAttribute('stroke', 'currentColor');
    svg.setAttribute('stroke-width', '1.5');
    svg.setAttribute('aria-hidden', 'true');
    const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
    path.setAttribute(
      'd',
      'M8 2v4m8-4v4M3 10h18M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z',
    );
    svg.append(path);
    const value = due.kind === 'date' ? due.date : due.local;
    const date = new Date(due.kind === 'date' ? `${value}T12:00:00` : value);
    const timestamp =
      due.kind === 'timed' ? zonedLocalTimestamp(due.local, due.timezone) : null;
    const text =
      due.kind === 'timed' && timestamp != null && formatPreferred
        ? formatPreferred(timestamp)
        : Number.isNaN(date.getTime())
          ? value
          : date.toLocaleString(undefined, {
              month: 'short',
              day: 'numeric',
              year: 'numeric',
              ...(due.kind === 'timed' ? { hour: 'numeric', minute: '2-digit' } : {}),
            });
    const dateLabel = document.createElement('span');
    dateLabel.className = 'task-date-summary';
    dateLabel.append(
      svg,
      document.createTextNode(`${schedule?.due ? 'Due' : 'Planned for'} ${text}`),
    );
    summary.append(dateLabel);
    dateLabel.title = formatPreferred?.timeZone || defaultZone;
    summary.title = 'Edit planning';
  }
  async function load(explicit = false) {
    if (!active || loading || busy) return;
    if (!explicit && (scheduleDraft || recurrenceDraft))
      say(
        'Your planning draft keeps its original revision. Live changes will not replace it.',
      );
    const version = ++loadVersion;
    loading = true;
    try {
      const result = await request(`/tasks/${encodeURIComponent(taskId)}/schedule`);
      if (!active || version !== loadVersion) return;
      storedSchedule = result;
      showPriorityPreview(result.schedule);
      if (!scheduleDraft) {
        baseRevision = result.task_revision;
        applyValues(fromSchedule(result.schedule));
      }
      if (!recurrenceDraft) recurrenceRevision = result.task_revision;
      if (!scheduleDraft && !recurrenceDraft)
        say(canWrite ? '' : 'This schedule is read only.');
      reminderGroup.open = Boolean(result.schedule.reminders?.length);
      updateDraftActions();
      setDisabled(false);
      if (scheduleDraft) queueAutoSave();
    } catch (e) {
      if (active) {
        say(`Could not load schedule: ${e.message}. Your draft is preserved.`, true);
        if (scheduleDraft || recurrenceDraft) setDisabled(false);
      }
    } finally {
      loading = false;
    }
  }
  function conflict(message) {
    reload.hidden = false;
    say(`${message} Your draft is preserved. Review saved data before replacing it.`, true);
    onConflict?.(new Error(message));
  }
  function ensureRevision(revision) {
    if (!Number.isInteger(revision) || getRevision?.() !== revision) {
      conflict('The task changed since this planning draft began.');
      return false;
    }
    return true;
  }
  async function saveSchedule(event) {
    event?.preventDefault();
    clearTimeout(autoSaveTimer);
    if (!active || busy || !canWrite || !scheduleDraft || !form.checkValidity()) return;
    const expected = baseRevision;
    if (!ensureRevision(expected)) return;
    const v = scheduleValues();
    const schedule = {
      planned: v.planned ? { kind: 'date', date: v.planned } : null,
      due:
        v.dueKind === 'date'
          ? { kind: 'date', date: v.dueDate }
          : v.dueKind === 'timed'
            ? { kind: 'timed', local: local(v.dueLocal), timezone: v.dueZone.trim() }
            : null,
      priority: v.priority ? Number(v.priority) : null,
      reminders: v.reminders.map((r) => ({
        channel: r.channel,
        at: { kind: 'timed', local: local(r.local), timezone: r.timezone.trim() },
      })),
    };
    busy = true;
    setDisabled(true);
    let release;
    try {
      release = beginWrite?.();
      const result = await request(
        `/tasks/${encodeURIComponent(taskId)}/schedule`,
        'PUT',
        { revision: expected, schedule },
      );
      if (!active) return;
      if (onSaved?.(expected, result.task_revision) === false) {
        conflict('The schedule was saved, but the open task has changed.');
        return;
      }
      baseRevision = result.task_revision;
      scheduleDraft = null;
      remove('schedule');
      storedSchedule = result;
      showDue(result.schedule);
      applyValues(fromSchedule(result.schedule));
      if (!recurrenceDraft) recurrenceRevision = result.task_revision;
      say('');
      updateDraftActions();
    } catch (e) {
      if (active) {
        reload.hidden = false;
        if (storedSchedule) showPriorityPreview(storedSchedule.schedule);
        say(
          `Schedule not confirmed: ${e.message}. Your draft is preserved. Reload saved data before retrying an ambiguous result.`,
          true,
        );
        if (e.status === 409) onConflict?.(e);
      }
    } finally {
      release?.();
      busy = false;
      if (active) setDisabled(false);
    }
  }
  async function createSeries(event) {
    event.preventDefault();
    if (!active || busy || !canWrite || !recurrence.reportValidity()) return;
    if (scheduleDraft) {
      say(
        form.checkValidity()
          ? 'Planning changes are still being saved. Wait a moment before starting a series.'
          : 'Complete the planning fields before starting a recurring series.',
      );
      return;
    }
    recurrenceChanged();
    const expected = recurrenceRevision;
    if (!ensureRevision(expected)) return;
    const v = recurrenceValues();
    if (!v.start || !v.timezone.trim()) {
      say('Choose a first due time and IANA timezone.');
      return;
    }
    if (v.rule === 'weekdays' && !v.weekdays.length) {
      say('Choose at least one weekday.');
      return;
    }
    const spec = {
      rule:
        v.rule === 'weekdays'
          ? { kind: 'weekdays', days: v.weekdays }
          : v.rule === 'completion_relative'
            ? { kind: 'completion_relative', days: Number(v.interval) }
            : { kind: 'daily', interval_days: Number(v.interval) },
      start_local: local(v.start),
      timezone: v.timezone.trim(),
      end_date: v.end || null,
      count: v.count ? Number(v.count) : null,
    };
    busy = true;
    setDisabled(true);
    let release;
    try {
      release = beginWrite?.();
      const series = await request(
        `/tasks/${encodeURIComponent(taskId)}/series`,
        'POST',
        { revision: expected, spec },
      );
      if (!active) return;
      if (onSaved?.(expected, expected + 1) === false) {
        conflict('The series was created, but the open task has changed.');
        return;
      }
      baseRevision = expected + 1;
      recurrenceRevision = expected + 1;
      recurrenceDraft = null;
      remove('recurrence');
      say('Recurring series created. Its first due time is now the task deadline.');
      seriesCreated = true;
      startSeries.disabled = true;
      seriesLink.href = `/recurring#${encodeURIComponent(series.id)}`;
      // Refresh only the clean saved schedule, not a draft.
      storedSchedule = {
        task_revision: expected + 1,
        schedule: {
          ...storedSchedule?.schedule,
          due: { kind: 'timed', local: spec.start_local, timezone: spec.timezone },
        },
      };
      applyValues(fromSchedule(storedSchedule.schedule));
    } catch (e) {
      if (active) {
        say(
          `Series creation not confirmed: ${e.message}. Your draft is preserved. Review existing series before retrying an ambiguous result.`,
          true,
        );
        if (e.status === 409) onConflict?.(e);
      }
    } finally {
      release?.();
      busy = false;
      if (active) setDisabled(false);
    }
  }
  const cleanup = () => {
    active = false;
    clearTimeout(autoSaveTimer);
    window.removeEventListener('online', queueAutoSave);
    loadVersion++;
    for (const controller of requests) controller.abort();
    requests.clear();
  };
  cleanup.refresh = () => {
    if (!active || busy) return;
    if (scheduleDraft || recurrenceDraft) {
      say('Task data changed. Your planning draft and original revision are preserved.');
      return;
    }
    load();
  };
  return cleanup;
}
