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
  const defaultZone = Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';
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
    ['', 'No priority'],
    ['1', 'High'],
    ['2', 'Medium'],
    ['3', 'Low'],
  ]);
  const reminders = el('div', { 'data-reminders': '' });
  const addReminder = el('button', { type: 'button', class: 'primary' }, 'Add reminder');
  const save = el('button', { type: 'submit', class: 'primary' }, 'Save schedule');
  const reload = el(
    'button',
    { type: 'button', class: 'primary' },
    'Load saved schedule',
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
    { type: 'button', class: 'primary' },
    'Discard recurrence draft',
  );
  const seriesLink = el(
    'a',
    { href: '/recurring', class: 'nav-link' },
    'Manage existing recurring series',
  );
  form.append(
    el('h3', {}, 'Schedule'),
    label('Planned date', planned),
    el(
      'p',
      { class: 'muted' },
      'Planning is a date only. It does not set a deadline or notification.',
    ),
    label('Due', dueKind),
    label('Due date', dueDate),
    label('Due local date and time', dueLocal),
    label('Due IANA timezone', dueZone),
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
      'Available for an unfinished root task with no subtasks. Starting a series sets this task’s due date and time to the start below. Save schedule edits first. Existing series can be paused or stopped on the Recurring page.',
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
  details.append(summary, notice, form, recurrence);
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
  reload.addEventListener('click', () => {
    if (busy) return;
    if (
      (scheduleDraft || recurrenceDraft) &&
      !confirm(
        'Discard the schedule and recurrence drafts for this task and load the saved schedule?',
      )
    )
      return;
    scheduleDraft = null;
    recurrenceDraft = null;
    remove('schedule');
    remove('recurrence');
    applyRecurrence({});
    load(true);
  });
  discardRecurrence.addEventListener('click', () => {
    if (busy || !recurrenceDraft) return;
    if (!confirm('Discard this recurrence draft?')) return;
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
  function say(text) {
    if (active) notice.textContent = text;
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
    const row = el('fieldset', { class: 'grid gap-3 mt-8' });
    row.append(el('legend', {}, 'Reminder'));
    const channel = select('channel', [
      ['email', 'Email'],
      ['push', 'Push notification'],
    ]);
    channel.value = values.channel || 'email';
    const at = input('datetime-local', 'reminder_local', values.local || '');
    at.required = true;
    const timezone = input('text', 'reminder_timezone', values.timezone || defaultZone);
    timezone.required = true;
    const remove = el('button', { type: 'button', class: 'primary' }, 'Remove reminder');
    remove.hidden = !canWrite;
    remove.addEventListener('click', () => {
      if (busy || !canWrite) return;
      row.remove();
      scheduleChanged();
    });
    row.append(
      label('Channel', channel),
      label('Reminder local date and time', at),
      label('Reminder IANA timezone', timezone),
      remove,
    );
    reminders.append(row);
  }
  function updateVisibility() {
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
  }
  function recurrenceChanged() {
    if (!active || busy || !canWrite) return;
    updateVisibility();
    if (!Number.isInteger(recurrenceRevision)) return;
    recurrenceDraft = { revision: recurrenceRevision, values: recurrenceValues() };
    persist('recurrence', recurrenceDraft);
  }
  async function request(path, method = 'GET', body) {
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
      priority: schedule.priority == null ? '' : String(schedule.priority),
      reminders: (schedule.reminders || []).map((r) => ({
        channel: r.channel,
        local: r.at.local,
        timezone: r.at.timezone,
      })),
    };
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
      if (!scheduleDraft) {
        baseRevision = result.task_revision;
        applyValues(fromSchedule(result.schedule));
      }
      if (!recurrenceDraft) recurrenceRevision = result.task_revision;
      if (!scheduleDraft && !recurrenceDraft)
        say(
          canWrite
            ? 'Schedule and task brief save separately. Local times require an IANA timezone; ambiguous or nonexistent times are rejected.'
            : 'This schedule is read only.',
        );
      setDisabled(false);
    } catch (e) {
      if (active) {
        say(`Could not load schedule: ${e.message}. Your draft is preserved.`);
        if (scheduleDraft || recurrenceDraft) setDisabled(false);
      }
    } finally {
      loading = false;
    }
  }
  function conflict(message) {
    say(`${message} Your draft is preserved. Review saved data before replacing it.`);
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
    event.preventDefault();
    if (!active || busy || !canWrite || !form.reportValidity()) return;
    scheduleChanged();
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
      applyValues(fromSchedule(result.schedule));
      if (!recurrenceDraft) recurrenceRevision = result.task_revision;
      say('Schedule saved.');
    } catch (e) {
      if (active) {
        say(
          `Schedule not confirmed: ${e.message}. Your draft is preserved. Reload saved data before retrying an ambiguous result.`,
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
      say('Save or discard your schedule draft before starting a series.');
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
