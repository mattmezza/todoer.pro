import { setSaveState } from './quiet-info.js';
import { confirmAction } from './confirm.js';
const main = document.querySelector('[data-workbench]');
const csrf = document.querySelector('meta[name=csrf-token]')?.content;
const zone = main?.dataset.timezone || 'UTC';
const content = main?.querySelector('[data-workbench-content]');
const toolbar = main?.querySelector('[data-workbench-toolbar]');
const status = main?.querySelector('[data-workbench-status]');
let inboxId,
  entries = [],
  generation = 0;
const node = (tag, text, cls) => {
  const n = document.createElement(tag);
  if (text) n.textContent = text;
  if (cls) n.className = cls;
  return n;
};
function button(text, action, cls) {
  const b = node('button', text, cls);
  b.type = 'button';
  b.onclick = () => action(b);
  return b;
}
async function api(path, method = 'GET', body, key) {
  const r = await fetch('/api/v1/' + path, {
    method,
    cache: 'no-store',
    headers: {
      'Content-Type': 'application/json',
      'X-CSRF-Token': csrf,
      ...(key ? { 'Idempotency-Key': key } : {}),
    },
    ...(body ? { body: JSON.stringify(body) } : {}),
  });
  const data = await r.json();
  if (!r.ok) throw Error(data.error?.message || 'Could not save. Try again.');
  return data;
}
function fail(error) {
  setSaveState('workbench', 'error', error.message);
  status.textContent = '';
}
async function write(b, action) {
  b.disabled = true;
  setSaveState('workbench', 'saving');
  try {
    await action();
    setSaveState('workbench', 'saved');
    await load();
    return true;
  } catch (e) {
    fail(e);
    return false;
  } finally {
    b.disabled = false;
  }
}
const today = new Intl.DateTimeFormat('en-CA', {
  timeZone: zone,
  year: 'numeric',
  month: '2-digit',
  day: '2-digit',
}).format(new Date());
function add(date, days) {
  const d = new Date(date + 'T12:00:00Z');
  d.setUTCDate(d.getUTCDate() + days);
  return d.toISOString().slice(0, 10);
}
function nextWeek() {
  const day = new Date(today + 'T12:00:00Z').getUTCDay() || 7;
  let delta = (Number(main.dataset.weekStart) - day + 7) % 7;
  return add(today, delta || 7);
}
function nextMonth() {
  const d = new Date(today + 'T12:00:00Z'),
    day = d.getUTCDate();
  d.setUTCDate(1);
  d.setUTCMonth(d.getUTCMonth() + 1);
  const last = new Date(
    Date.UTC(d.getUTCFullYear(), d.getUTCMonth() + 1, 0),
  ).getUTCDate();
  d.setUTCDate(Math.min(day, last));
  return d.toISOString().slice(0, 10);
}
let query = new URLSearchParams(location.search),
  view = query.get('view') || 'week',
  anchor = query.get('date') || today;
let from, to;
function range() {
  if (view === 'custom') {
    from = query.get('from') || today;
    to = query.get('to') || add(from, 6);
  } else if (view === 'day') {
    from = to = anchor;
  } else if (view === 'month') {
    const day = new Date(anchor + 'T12:00:00Z');
    from = anchor.slice(0, 7) + '-01';
    to = new Date(Date.UTC(day.getUTCFullYear(), day.getUTCMonth() + 1, 0, 12))
      .toISOString()
      .slice(0, 10);
  } else if (view === 'workweek' || view === 'weekend') {
    const weekday = new Date(anchor + 'T12:00:00Z').getUTCDay() || 7;
    const monday = add(anchor, 1 - weekday);
    from = view === 'workweek' ? monday : add(monday, 5);
    to = add(from, view === 'workweek' ? 4 : 1);
  } else {
    const day = new Date(anchor + 'T12:00:00Z').getUTCDay() || 7;
    from = add(anchor, -((day - Number(main.dataset.weekStart) + 7) % 7));
    to = add(from, 6);
  }
}
function syncRange() {
  range();
  const q = new URLSearchParams({
    view,
    date: anchor,
    ...(view === 'custom' ? { from, to } : {}),
  });
  history.replaceState(null, '', '?' + q);
}
function dateInput(value) {
  const n = node('input');
  n.type = 'date';
  n.value = value;
  return n;
}
function modal(title) {
  const dialog = node('dialog', null, 'quick-dialog workbench-dialog');
  const h = node('h2', title);
  const close = button('', () => dialog.close(), 'workbench-dialog-close');
  close.innerHTML =
    '<svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#close"/></svg>';
  close.setAttribute('aria-label', 'Close dialog');
  dialog.append(h, close);
  let backdropPress = false;
  const outside = (event) => {
    const r = dialog.getBoundingClientRect();
    return (
      event.target === dialog &&
      (event.clientX < r.left ||
        event.clientX > r.right ||
        event.clientY < r.top ||
        event.clientY > r.bottom)
    );
  };
  dialog.addEventListener('pointerdown', (event) => (backdropPress = outside(event)));
  dialog.addEventListener('click', (event) => {
    if (backdropPress && outside(event)) dialog.close();
    backdropPress = false;
  });
  dialog.addEventListener('close', () => dialog.remove(), { once: true });
  document.body.append(dialog);
  dialog.showModal();
  return dialog;
}
async function placement(entry, b, date, time = null) {
  return write(b, () =>
    api(`tasks/${encodeURIComponent(entry.task.id)}/placement`, 'PUT', {
      revision: entry.task.revision,
      date,
      time,
      timezone: zone,
    }),
  );
}
function planDialog(entry, initial) {
  const d = modal('Plan this task');
  d.append(node('p', entry.task.title));
  const form = node('form');
  const date = dateInput(initial || entry.schedule.planned?.date || today);
  date.required = true;
  const time = node('input');
  time.type = 'time';
  time.value = entry.local_time || '';
  const dateLabel = node('label', 'Day');
  dateLabel.append(date);
  const timeLabel = node('label', 'Time (optional)');
  timeLabel.append(time);
  const submit = node('button', 'Save', 'primary');
  submit.type = 'submit';
  const fields = node('div', null, 'workbench-date-fields');
  fields.append(dateLabel, timeLabel);
  const actions = node('div', null, 'workbench-form-actions');
  actions.append(
    button('Remove from plan', async (b) => {
      if (await placement(entry, b, null)) d.close();
    }),
    submit,
  );
  form.append(fields, actions);
  form.onsubmit = async (e) => {
    e.preventDefault();
    if (await placement(entry, submit, date.value, time.value || null)) d.close();
  };
  d.append(form);
}
async function organize(entry) {
  const d = modal('Move to a list');
  d.append(node('p', entry.task.title));
  const input = node('input');
  input.type = 'search';
  input.placeholder = 'Search lists or spaces…';
  input.setAttribute('aria-label', 'Search lists or spaces');
  const results = node('div', null, 'destination-results');
  d.append(input, results);
  let version = 0,
    timer;
  async function search() {
    const v = ++version;
    try {
      const r = await api('triage/destinations?q=' + encodeURIComponent(input.value));
      if (v !== version || !d.isConnected) return;
      results.replaceChildren();
      for (const list of r.choices) {
        results.append(
          button(
            `${list.space ? '@' + list.space + ' / ' : ''}#${list.title}`,
            async (b) => {
              await write(b, async () => {
                const preview = await api(
                  `tasks/${encodeURIComponent(entry.task.id)}/move-preview?destination=${encodeURIComponent(list.id)}`,
                );
                if (
                  preview.audience_broadens &&
                  !(await confirmAction(
                    preview.destination_public && !preview.source_public
                      ? 'Moving this task and its descendants makes them publicly viewable. Continue?'
                      : 'Moving this task and its descendants gives additional members access. Continue?',
                  ))
                )
                  return;
                await api(`tasks/${encodeURIComponent(entry.task.id)}/move`, 'POST', {
                  destination: list.id,
                  revision: entry.task.revision,
                  acknowledge_access_change: !!preview.audience_broadens,
                });
                d.close();
              });
            },
            'destination-choice',
          ),
        );
      }
      if (!r.choices.length) results.append(node('p', 'No matching lists.', 'muted'));
    } catch (e) {
      fail(e);
    }
  }
  input.oninput = () => {
    clearTimeout(timer);
    timer = setTimeout(search, 180);
  };
  await search();
  input.focus();
  try {
    const enabled = await api('assist/status');
    if (enabled.enabled)
      d.append(
        button('Suggest a destination', async (b) => {
          b.disabled = true;
          try {
            const r = await api('assist/classify', 'POST', {
              idempotency_key: crypto.randomUUID(),
              task_id: entry.task.id,
              revision: entry.task.revision,
            });
            if (r.destination) {
              input.value = r.destination.title;
              await search();
            } else
              results.prepend(
                node(
                  'p',
                  'No suggestion available. Try searching a list or space.',
                  'muted',
                ),
              );
          } catch (e) {
            fail(e);
          } finally {
            b.disabled = false;
          }
        }),
      );
  } catch {}
}
function taskCard(entry, intake = false) {
  const t = entry.task;
  const row = node('article', null, 'workbench-task');
  row.dataset.taskId = t.id;
  row.dataset.planningTask = t.id;
  const check = button(
    t.complete ? '✓' : '',
    (b) =>
      write(b, () =>
        api(`tasks/${encodeURIComponent(t.id)}`, 'PATCH', {
          revision: t.revision,
          complete: !t.complete,
        }),
      ),
    'workbench-check',
  );
  check.setAttribute('aria-label', t.complete ? 'Mark incomplete' : 'Complete task');
  check.setAttribute('aria-pressed', String(t.complete));
  const text = node('div', null, 'workbench-task-copy');
  const link = node('a', t.title);
  link.href = '/tasks/' + encodeURIComponent(t.id);
  if (t.complete) link.className = 'is-complete';
  text.append(link);
  if (!intake)
    text.append(
      node(
        'small',
        `${entry.space ? '@' + entry.space + ' / ' : ''}#${entry.list}`,
        'muted',
      ),
    );
  row.append(check, text);
  if (intake) {
    const actions = node('div', null, 'inbox-quick-actions');
    for (const [label, date] of [
      ['Today', today],
      ['Tomorrow', add(today, 1)],
      ['Next week', nextWeek()],
      ['In 2 weeks', add(today, 14)],
      ['Next month', nextMonth()],
    ])
      actions.append(button(label, (b) => placement(entry, b, date)));
    actions.append(
      button('Choose date…', () => planDialog(entry)),
      button('Move to list…', () => organize(entry)),
    );
    row.append(actions);
  } else {
    row.append(
      button(entry.local_time || 'All day', () => planDialog(entry), 'plan-time-action'),
    );
  }
  return row;
}
function renderInbox() {
  if (!toolbar.querySelector('.inbox-capture')) {
    const form = node('form', null, 'inbox-capture');
    const input = node('input');
    input.placeholder = 'Capture a task…';
    input.setAttribute('aria-label', 'Task title');
    input.maxLength = 500;
    input.required = true;
    const submit = node('button', 'Add task', 'primary');
    submit.type = 'submit';
    form.append(input, submit);
    let key = crypto.randomUUID();
    form.onsubmit = async (e) => {
      e.preventDefault();
      const title = input.value.trim();
      if (!title) return;
      await write(submit, async () => {
        await api(
          'capture',
          'POST',
          { list_id: inboxId, tasks: [{ title, description: '' }] },
          key,
        );
        key = crypto.randomUUID();
        input.value = '';
      });
    };
    toolbar.append(form);
  }
  content.replaceChildren(...entries.map((e) => taskCard(e, true)));
  if (!entries.length) {
    const empty = node('div', null, 'planning-empty');
    empty.append(
      node('h2', 'A clear place to start.'),
      node(
        'p',
        'New ideas land here. Plan a day or move them to a list when you’re ready.',
      ),
    );
    content.append(empty);
  }
}
function addPlannedTask(date, initialTime = '') {
  const d = modal('Add to ' + labelDate(date));
  const form = node('form'),
    title = node('input'),
    time = node('input');
  title.placeholder = 'What would you like to do?';
  title.required = true;
  title.maxLength = 500;
  title.setAttribute('aria-label', 'Task title');
  time.type = 'time';
  time.value = initialTime;
  time.setAttribute('aria-label', 'Time (optional)');
  const timeLabel = node('label', 'Time (optional)');
  timeLabel.append(time);
  const submit = node('button', 'Add task', 'primary');
  submit.type = 'submit';
  form.append(title, timeLabel, submit);
  d.append(form);
  title.focus();
  const key = crypto.randomUUID();
  let captured;
  form.onsubmit = async (event) => {
    event.preventDefault();
    await write(submit, async () => {
      if (!captured) {
        const intake = await api('inbox');
        const rows = await api(
          'capture',
          'POST',
          {
            list_id: intake.list_id,
            tasks: [{ title: title.value.trim(), description: '' }],
          },
          key,
        );
        captured = rows[0];
        title.disabled = true;
      }
      await api(`tasks/${encodeURIComponent(captured.id)}/placement`, 'PUT', {
        revision: captured.revision,
        date,
        time: time.value || null,
        timezone: zone,
      });
      d.close();
    });
  };
}
function labelDate(date) {
  return new Intl.DateTimeFormat(undefined, {
    weekday: 'short',
    month: 'short',
    day: 'numeric',
    timeZone: 'UTC',
  }).format(new Date(date + 'T12:00:00Z'));
}
function renderPlan() {
  toolbar.replaceChildren();
  const controls = node('div', null, 'plan-controls');
  for (const [v, label] of [
    ['day', 'Day'],
    ['workweek', 'Workweek'],
    ['weekend', 'Weekend'],
    ['week', 'Week'],
    ['month', 'Month'],
    ['custom', 'Custom range'],
  ]) {
    const b = button(label, async () => {
      view = v;
      syncRange();
      await load();
    });
    b.setAttribute('aria-pressed', String(v === view));
    controls.append(b);
  }
  controls.append(
    button('←', async () => {
      navigate(-1);
      syncRange();
      await load();
    }),
    button('Today', async () => {
      anchor = today;
      syncRange();
      await load();
    }),
    button('→', async () => {
      navigate(1);
      syncRange();
      await load();
    }),
  );
  if (view === 'custom') {
    const f = dateInput(from),
      t = dateInput(to);
    f.setAttribute('aria-label', 'Start date');
    t.setAttribute('aria-label', 'End date');
    controls.append(
      f,
      t,
      button('Apply', async () => {
        query.set('from', f.value);
        query.set('to', t.value);
        syncRange();
        await load();
      }),
    );
  } else {
    const date = dateInput(anchor);
    date.setAttribute('aria-label', 'Go to date');
    date.onchange = async () => {
      anchor = date.value;
      syncRange();
      await load();
    };
    controls.append(date);
  }
  const caption = node(
    'h2',
    new Intl.DateTimeFormat(undefined, {
      month: 'long',
      year: 'numeric',
      timeZone: 'UTC',
    }).format(new Date(anchor + 'T12:00:00Z')),
    'calendar-caption',
  );
  toolbar.append(caption, controls);
  const done = entries.filter((e) => e.task.complete).length,
    total = entries.length,
    percent = total ? Math.round((done / total) * 100) : 0;
  const summary = node('div', null, 'plan-summary');
  summary.append(
    node(
      'strong',
      total
        ? done === total
          ? view === 'week'
            ? 'You’re done for the week!'
            : 'Everything planned is complete!'
          : `You’re ${percent}% through ${view === 'day' ? 'your day' : 'this plan'}.`
        : 'Room for what matters.',
    ),
    node('span', `${done}/${total} tasks complete`, 'muted'),
  );
  const progress = node('progress');
  progress.max = total || 1;
  progress.value = done;
  progress.setAttribute('aria-label', 'Plan completion');
  summary.append(progress);
  toolbar.append(summary);
  content.replaceChildren(view === 'month' ? monthBoard() : timeBoard());
}
function navigate(direction) {
  if (view === 'month') {
    const date = new Date(from + 'T12:00:00Z');
    date.setUTCMonth(date.getUTCMonth() + direction);
    anchor = date.toISOString().slice(0, 10);
  } else anchor = add(anchor, direction * (view === 'day' ? 1 : 7));
}
function openDay(date) {
  view = 'day';
  anchor = date;
  syncRange();
  load();
}
function dayEntries(date) {
  return entries
    .filter((entry) => entry.schedule.planned?.date === date)
    .sort((a, b) => (a.local_time || '').localeCompare(b.local_time || ''));
}
function calendarDayTitle(date) {
  const header = node('div', null, 'calendar-day-heading');
  if (date === today) header.classList.add('is-today');
  const day = new Date(date + 'T12:00:00Z');
  header.append(
    node(
      'span',
      new Intl.DateTimeFormat(undefined, { weekday: 'short', timeZone: 'UTC' }).format(
        day,
      ),
    ),
    button(String(day.getUTCDate()), () => openDay(date)),
  );
  return header;
}
function timeBoard() {
  const dates = [];
  for (let date = from; date <= to; date = add(date, 1)) dates.push(date);
  const shell = node('div', null, 'calendar-shell');
  const board = node('div', null, 'calendar-time-board');
  board.style.setProperty('--calendar-days', dates.length);
  const corner = node('div', null, 'calendar-corner');
  corner.title = zone;
  corner.textContent = '';
  board.append(corner);
  for (const date of dates) board.append(calendarDayTitle(date));
  board.append(node('div', 'All day', 'calendar-all-day-label'));
  for (const date of dates) {
    const area = node('div', null, 'calendar-all-day');
    area.dataset.calendarDate = date;
    for (const entry of dayEntries(date).filter((entry) => !entry.local_time)) {
      const card = taskCard(entry);
      card.classList.add('calendar-event');
      area.append(card);
    }
    const addButton = button('+', () => addPlannedTask(date), 'calendar-add');
    addButton.setAttribute('aria-label', 'Add task on ' + labelDate(date));
    area.append(addButton);
    board.append(area);
  }
  const hours = node('div', null, 'calendar-hours');
  for (let hour = 0; hour < 24; hour++) {
    const label = node('span', String(hour).padStart(2, '0') + ':00');
    label.style.top = hour * 56 + 'px';
    hours.append(label);
  }
  board.append(hours);
  for (const date of dates) {
    const track = node('div', null, 'calendar-day-track');
    track.dataset.calendarDate = date;
    if (date === today) track.classList.add('is-today');
    for (let hour = 0; hour < 24; hour++) {
      const slot = button(
        '',
        () => addPlannedTask(date, String(hour).padStart(2, '0') + ':00'),
        'calendar-hour-slot',
      );
      slot.setAttribute(
        'aria-label',
        `Add task on ${labelDate(date)} at ${String(hour).padStart(2, '0')}:00`,
      );
      track.append(slot);
    }
    const timed = dayEntries(date).filter((entry) => entry.local_time);
    for (const entry of timed) {
      const [hour, minute] = entry.local_time.split(':').map(Number);
      const card = taskCard(entry);
      card.classList.add('calendar-event', 'calendar-timed-event');
      card.style.top = hour * 56 + (minute / 60) * 56 + 'px';
      track.append(card);
    }
    if (date === today) {
      const parts = new Intl.DateTimeFormat('en-GB', {
        timeZone: zone,
        hour: '2-digit',
        minute: '2-digit',
        hourCycle: 'h23',
      })
        .format(new Date())
        .split(':')
        .map(Number);
      const now = node('div', null, 'calendar-now');
      now.style.top = parts[0] * 56 + (parts[1] / 60) * 56 + 'px';
      now.setAttribute('aria-label', 'Current time');
      track.append(now);
    }
    board.append(track);
  }
  shell.append(board);
  requestAnimationFrame(() => {
    if (shell.isConnected) shell.scrollTop = 8 * 56;
  });
  return shell;
}
function monthBoard() {
  const board = node('div', null, 'calendar-month-board');
  const start = Number(main.dataset.weekStart);
  for (let i = 0; i < 7; i++) {
    const date = add('2026-10-26', (start - 1 + i) % 7);
    board.append(
      node(
        'div',
        new Intl.DateTimeFormat(undefined, { weekday: 'short', timeZone: 'UTC' }).format(
          new Date(date + 'T12:00:00Z'),
        ),
        'calendar-month-weekday',
      ),
    );
  }
  const weekday = new Date(from + 'T12:00:00Z').getUTCDay() || 7;
  const begin = add(from, -((weekday - start + 7) % 7));
  const end = add(
    to,
    (start + 6 - (new Date(to + 'T12:00:00Z').getUTCDay() || 7) + 7) % 7,
  );
  for (let date = begin; date <= end; date = add(date, 1)) {
    const cell = node('section', null, 'calendar-month-day');
    if (date < from || date > to) cell.classList.add('outside-month');
    if (date === today) cell.classList.add('is-today');
    const heading = button(
      String(new Date(date + 'T12:00:00Z').getUTCDate()),
      () => openDay(date),
      'calendar-month-date',
    );
    heading.setAttribute('aria-label', labelDate(date));
    cell.append(heading);
    const list = dayEntries(date);
    for (const entry of list.slice(0, 3)) {
      const link = node(
        'a',
        (entry.local_time ? entry.local_time + ' ' : '') + entry.task.title,
        'calendar-month-task',
      );
      link.href = '/tasks/' + encodeURIComponent(entry.task.id);
      cell.append(link);
    }
    if (list.length > 3)
      cell.append(
        button(`+${list.length - 3} more`, () => openDay(date), 'calendar-more'),
      );
    const addButton = button('+', () => addPlannedTask(date), 'calendar-add');
    addButton.setAttribute('aria-label', 'Add task on ' + labelDate(date));
    cell.append(addButton);
    board.append(cell);
  }
  return board;
}

async function load() {
  const version = ++generation;
  status.textContent = 'Loading…';
  try {
    const intake = main.dataset.workbench === 'inbox';
    if (!intake) range();
    const data = await api(intake ? 'inbox' : `plan?from=${from}&to=${to}`);
    if (version !== generation) return;
    entries = data.entries;
    inboxId = data.list_id;
    status.textContent = '';
    if (intake) renderInbox();
    else renderPlan();
  } catch (e) {
    if (version === generation) fail(e);
  }
}
if (main) {
  main.dataset.planningToday = today;
  content.dataset.planningRows = '';
  import('./planning-page.js');
  document.addEventListener('todoer:workbench-refresh', load);
  main.querySelector('[data-export-workbench]').onclick = () => {
    const scope =
      main.dataset.workbench === 'inbox' ? { kind: 'inbox' } : { kind: 'plan', from, to };
    import('./export-menu.js').then((m) => m.openExport(scope)).catch(fail);
  };
  if (main.dataset.workbench === 'plan') syncRange();
  load();
  document.addEventListener('todoer:capture', load);
}
