import { inference } from './ai-transaction.js';
import { beginLoading } from './loading.js';
import { request as fetch } from './request.js';
import { setSaveState } from './quiet-info.js';
import { confirmAction } from './confirm.js';
const main = document.querySelector('[data-workbench]');
const csrf = document.querySelector('meta[name=csrf-token]')?.content;
const zone = main?.dataset.timezone || 'UTC';
const content = main?.querySelector('[data-workbench-content]');
const toolbar = main?.querySelector('[data-workbench-toolbar]');
const status = main?.querySelector('[data-workbench-status]');
let pointerHeld = false,
  deferredRefresh = false;
const pendingSettles = new Set();
function resumeRefresh() {
  if (
    deferredRefresh &&
    !pointerHeld &&
    pendingSettles.size === 0 &&
    !main?.querySelector('input:focus,textarea:focus,select:focus')
  ) {
    deferredRefresh = false;
    load();
  }
}
document.addEventListener(
  'pointerdown',
  (event) => {
    pointerHeld = Boolean(main?.contains(event.target));
  },
  true,
);
for (const type of ['pointerup', 'pointercancel'])
  document.addEventListener(
    type,
    () => {
      pointerHeld = false;
      setTimeout(resumeRefresh, 0);
    },
    true,
  );
main?.addEventListener('focusout', () => setTimeout(resumeRefresh, 0));
document.addEventListener('click', (event) => {
  for (const menu of main?.querySelectorAll('.inbox-plan-menu[open]') || [])
    if (!menu.contains(event.target)) menu.open = false;
});
document.addEventListener('keydown', (event) => {
  if (event.key !== 'Escape') return;
  const menu = main?.querySelector('.inbox-plan-menu[open]');
  if (!menu || document.querySelector('dialog[open]')) return;
  event.preventDefault();
  event.stopImmediatePropagation();
  menu.open = false;
  menu.querySelector('summary').focus();
});
let inboxId,
  entries = [],
  generation = 0,
  inboxNext = null,
  inboxRevision = 0,
  morePending = false,
  moreObserver,
  rollObserver,
  rollFrame;
const inboxPageSize = 40;
const visibleInboxRows = new Set();
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
  if (path === 'assist/classify' || path === 'assist/drafts')
    return inference(path.slice(7), body, csrf);
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
  if (b.disabled) return false;
  const scope = b.closest('dialog, article, form') || b;
  if (scope.getAttribute('aria-busy') === 'true') return false;
  const controls = [...scope.querySelectorAll('button,input,select,textarea')];
  if (!controls.includes(b)) controls.push(b);
  const prior = controls.map((control) => [control, control.disabled]);
  scope.setAttribute('aria-busy', 'true');
  for (const [control] of prior) control.disabled = true;
  setSaveState('workbench', 'saving');
  try {
    if ((await action()) === false) {
      setSaveState('workbench', 'info');
      return false;
    }
    setSaveState('workbench', 'saved');
    await load();
    return true;
  } catch (e) {
    fail(e);
    return false;
  } finally {
    scope.removeAttribute('aria-busy');
    for (const [control, disabled] of prior) control.disabled = disabled;
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
  dialog.setAttribute('aria-label', title);
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
  d.classList.add('destination-dialog');
  d.append(node('h3', entry.task.title, 'destination-task-title'));
  const input = node('input');
  input.type = 'search';
  input.placeholder = 'Search lists or spaces…';
  input.setAttribute('aria-label', 'Search lists or spaces');
  const results = node('div', null, 'destination-results');
  const label = node('label', 'Choose a destination', 'destination-search-label');
  label.append(input);
  d.append(label, results);
  let version = 0,
    timer;
  async function search() {
    const v = ++version;
    const skeleton = beginLoading(results);
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
                  return false;
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
      if (!r.choices.length) {
        const empty = node('div', null, 'destination-empty');
        empty.setAttribute('role', 'status');
        empty.append(
          node('strong', 'No lists found'),
          node('p', 'Try a different list or space name.'),
        );
        results.append(empty);
      }
    } catch (e) {
      if (v === version) skeleton.fail(e.message, search);
      fail(e);
    } finally {
      skeleton.finish();
    }
  }
  input.oninput = () => {
    version++;
    beginLoading(results);
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
    row.classList.add('inbox-task');
    if (rollObserver) rollObserver.observe(row);
    else visibleInboxRows.add(row);
    if (t.description) text.append(node('p', t.description, 'inbox-description'));
    const meta = node('div', null, 'inbox-meta');
    if (t.list_id !== inboxId)
      meta.append(
        node('span', `${entry.space ? '@' + entry.space + ' / ' : ''}#${entry.list}`),
      );
    else meta.append(node('span', 'Inbox', 'inbox-location'));
    if (entry.schedule.planned)
      meta.append(
        node(
          'span',
          'Planned · ' +
            (entry.schedule.planned.date || entry.schedule.planned.local?.slice(0, 10)),
        ),
      );
    if (entry.schedule.due)
      meta.append(
        node(
          'span',
          'Due · ' + (entry.schedule.due.date || entry.schedule.due.local?.slice(0, 10)),
        ),
      );
    for (const tag of entry.tags || []) meta.append(node('span', '+' + tag, 'inbox-tag'));
    if (entry.schedule.priority != null) {
      const priority = node('span', 'P' + entry.schedule.priority, 'inbox-priority');
      priority.dataset.priority = entry.schedule.priority;
      meta.append(priority);
    }
    if (meta.childNodes.length) text.append(meta);
    const actions = node('div', null, 'inbox-quick-actions');
    const plan = node('details', null, 'inbox-plan-menu');
    plan.append(node('summary', 'Plan…'));
    const choices = node('div', null, 'inbox-plan-choices');
    for (const [label, date] of [
      ['Today', today],
      ['Tomorrow', add(today, 1)],
      ['Next week', nextWeek()],
      ['In 2 weeks', add(today, 14)],
      ['Next month', nextMonth()],
    ])
      choices.append(button(label, (b) => placement(entry, b, date)));
    choices.append(button('Choose date…', () => planDialog(entry)));
    plan.append(choices);
    actions.append(
      plan,
      button('Move…', () => organize(entry)),
      button('Settle', (b) => settle(entry, row, b), 'inbox-settle'),
    );
    row.append(actions);
  } else {
    row.append(
      button(entry.local_time || 'All day', () => planDialog(entry), 'plan-time-action'),
    );
  }
  return row;
}
const undoNotices = [];
let displayedUndo = null,
  leavingUndo = null,
  undoSequence = 0;
function dismissUndo(notice) {
  const index = undoNotices.indexOf(notice);
  if (index < 0) return;
  undoNotices.splice(index, 1);
  if (displayedUndo === notice) showLatestUndo();
}
function showLatestUndo() {
  const sequence = ++undoSequence;
  if (leavingUndo) {
    leavingUndo.animation?.cancel();
    leavingUndo.element.remove();
    leavingUndo = null;
  }
  const previous = displayedUndo;
  displayedUndo = null;
  const show = () => {
    if (sequence !== undoSequence) return;
    const next = undoNotices.at(-1);
    if (!next) return;
    displayedUndo = next;
    next.element.inert = false;
    next.element.removeAttribute('aria-hidden');
    main.append(next.element);
    if (!matchMedia('(prefers-reduced-motion: reduce)').matches)
      next.element.animate(
        [
          { opacity: 0, transform: 'translate(-50%, 20px)' },
          { opacity: 1, transform: 'translate(-50%, 0)' },
        ],
        { duration: 180, easing: 'ease-out' },
      );
  };
  if (!previous) {
    show();
    return;
  }
  previous.element.inert = true;
  previous.element.setAttribute('aria-hidden', 'true');
  const currentStyle = getComputedStyle(previous.element);
  const startOpacity = currentStyle.opacity;
  const startTransform = currentStyle.transform;
  for (const running of previous.element.getAnimations()) running.cancel();
  const animation = !matchMedia('(prefers-reduced-motion: reduce)').matches
    ? previous.element.animate(
        [
          { opacity: startOpacity, transform: startTransform },
          { opacity: 0, transform: 'translate(-50%, 20px)' },
        ],
        { duration: 140, easing: 'ease-in', fill: 'forwards' },
      )
    : null;
  leavingUndo = { element: previous.element, animation };
  const finish = () => {
    previous.element.remove();
    if (sequence !== undoSequence) return;
    leavingUndo = null;
    show();
  };
  if (animation) animation.finished.then(finish).catch(() => {});
  else finish();
}
async function settle(entry, row, b) {
  b.disabled = true;
  try {
    await api(`tasks/${encodeURIComponent(entry.task.id)}/inbox`, 'PUT', {
      settled: true,
    });
    const previousIndex = entries.findIndex((item) => item.task.id === entry.task.id);
    const nextId = entries[previousIndex + 1]?.task.id;
    entries = entries.filter((item) => item.task.id !== entry.task.id);
    inboxRevision++;
    pendingSettles.add(entry.task.id);
    let restored = false;
    row.style.overflow = 'hidden';
    const animation = !matchMedia('(prefers-reduced-motion: reduce)').matches
      ? row.animate(
          [
            { height: row.offsetHeight + 'px', opacity: 1 },
            {
              height: '0px',
              paddingTop: '0px',
              paddingBottom: '0px',
              marginTop: '0px',
              marginBottom: '0px',
              borderWidth: '0px',
              opacity: 0,
              transform: 'translateX(16px)',
            },
          ],
          { duration: 320, easing: 'cubic-bezier(.2,.7,.2,1)', fill: 'forwards' },
        )
      : null;
    const undo = node('div', null, 'inbox-undo');
    undo.setAttribute('role', 'status');
    undo.append(node('span', 'Settled · ' + entry.task.title));
    const notice = { element: undo };
    const restore = button('Undo', async () => {
      restore.disabled = true;
      try {
        await api(`tasks/${encodeURIComponent(entry.task.id)}/inbox`, 'PUT', {
          settled: false,
        });
        const collapsedHeight = row.isConnected ? row.getBoundingClientRect().height : 0;
        restored = true;
        inboxRevision++;
        animation?.cancel();
        pendingSettles.delete(entry.task.id);
        const current = entries.findIndex((item) => item.task.id === entry.task.id);
        if (current < 0) {
          const nextIndex = entries.findIndex((item) => item.task.id === nextId);
          entries.splice(
            nextIndex < 0 ? Math.min(previousIndex, entries.length) : nextIndex,
            0,
            entry,
          );
        }
        let restoredRow = [...content.querySelectorAll('.inbox-task')].find(
          (item) => item.dataset.taskId === entry.task.id,
        );
        if (!restoredRow) {
          restoredRow = taskCard(entry, true);
          const following = [...content.querySelectorAll('.inbox-task')].find(
            (item) => item.dataset.taskId === nextId,
          );
          content.querySelector('.planning-empty')?.remove();
          content.insertBefore(
            restoredRow,
            following || content.querySelector('[data-inbox-more]'),
          );
        }
        if (!matchMedia('(prefers-reduced-motion: reduce)').matches) {
          restoredRow.style.overflow = 'hidden';
          const expandedHeight = restoredRow.offsetHeight;
          const style = getComputedStyle(restoredRow);
          const returning = restoredRow.animate(
            [
              {
                height: collapsedHeight + 'px',
                paddingTop: '0px',
                paddingBottom: '0px',
                marginTop: '0px',
                marginBottom: '0px',
                borderWidth: '0px',
                opacity: 0,
              },
              {
                height: expandedHeight + 'px',
                paddingTop: style.paddingTop,
                paddingBottom: style.paddingBottom,
                marginTop: style.marginTop,
                marginBottom: style.marginBottom,
                borderWidth: style.borderTopWidth,
                opacity: 1,
              },
            ],
            { duration: 300, easing: 'cubic-bezier(.2,.7,.2,1)' },
          );
          returning.finished.finally(() => (restoredRow.style.overflow = ''));
        } else restoredRow.style.overflow = '';
        restoredRow.querySelector('.inbox-settle').disabled = false;
        decorateInbox();
        restoredRow.querySelector('.inbox-settle').focus();
        dismissUndo(notice);
        resumeRefresh();
      } catch (error) {
        restore.disabled = false;
        fail(error);
      }
    });
    undo.append(restore);
    undoNotices.push(notice);
    showLatestUndo();
    const finish = () => {
      if (restored) return;
      row.remove();
      rollObserver?.unobserve(row);
      visibleInboxRows.delete(row);
      pendingSettles.delete(entry.task.id);
      if (!entries.length && !inboxNext) renderInbox();
      else {
        decorateInbox();
        if (!entries.length && inboxNext) loadMore();
      }
      updateInboxFade();
      resumeRefresh();
    };
    if (animation) animation.finished.then(finish).catch(() => {});
    else finish();
  } catch (error) {
    b.disabled = false;
    fail(error);
  }
}
function inboxGroup(timestamp) {
  if (!timestamp) return 'Earlier';
  const date = new Date(timestamp * 1000);
  const local = new Intl.DateTimeFormat('en-CA', {
    timeZone: zone,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
  }).format(date);
  const days = Math.round(
    (Date.parse(today + 'T12:00:00Z') - Date.parse(local + 'T12:00:00Z')) / 86400000,
  );
  if (days <= 0) {
    if (Date.now() - date.getTime() < 3600000) return 'Just now';
    const hour = Number(
      new Intl.DateTimeFormat('en-GB', {
        timeZone: zone,
        hour: '2-digit',
        hourCycle: 'h23',
      }).format(date),
    );
    return hour < 12 ? 'This morning' : 'Earlier today';
  }
  if (days === 1) return 'Yesterday';
  if (days === 2) return 'Two days ago';
  const weekday = new Date(today + 'T12:00:00Z').getUTCDay() || 7;
  const weekOffset = (weekday - Number(main.dataset.weekStart) + 7) % 7;
  if (days <= weekOffset) return 'Earlier this week';
  if (days < weekOffset + 7)
    return (
      'Last ' +
      new Intl.DateTimeFormat('en', { weekday: 'long', timeZone: 'UTC' }).format(
        new Date(local + 'T12:00:00Z'),
      )
    );
  if (local.slice(0, 7) === today.slice(0, 7)) return 'Earlier this month';
  return new Intl.DateTimeFormat('en', {
    month: 'long',
    year: 'numeric',
    timeZone: 'UTC',
  }).format(new Date(local + 'T12:00:00Z'));
}
function gapText(seconds) {
  const hours = Math.floor(seconds / 3600);
  if (hours < 24) return `${hours} hours between captures`;
  const days = Math.floor(hours / 24);
  if (days < 14) return `${days} ${days === 1 ? 'day' : 'days'} between captures`;
  const weeks = Math.floor(days / 7);
  if (weeks < 8) return `${weeks} weeks between captures`;
  const months = Math.floor(days / 30);
  return `${months} months between captures`;
}
function decorateInbox() {
  content
    .querySelectorAll('.inbox-group-heading, .inbox-gap')
    .forEach((item) => item.remove());
  const byId = new Map(entries.map((entry) => [entry.task.id, entry]));
  let previous, group;
  for (const row of content.querySelectorAll('.inbox-task')) {
    const entry = byId.get(row.dataset.taskId);
    if (!entry) continue;
    if (
      previous?.created_at &&
      entry.created_at &&
      previous.created_at - entry.created_at >= 6 * 3600
    ) {
      const gap = node(
        'div',
        gapText(previous.created_at - entry.created_at),
        'inbox-gap',
      );
      gap.setAttribute('role', 'separator');
      content.insertBefore(gap, row);
    }
    const label = inboxGroup(entry.created_at);
    if (label !== group) {
      group = label;
      content.insertBefore(node('h2', label, 'inbox-group-heading'), row);
    }
    previous = entry;
  }
}
function updateInboxFade() {
  if (main?.dataset.workbench !== 'inbox') return;
  main.classList.toggle(
    'inbox-has-more-below',
    document.documentElement.scrollHeight - (window.scrollY + window.innerHeight) > 8,
  );
  scheduleInboxRoll();
}
function scheduleInboxRoll() {
  if (rollFrame) return;
  rollFrame = requestAnimationFrame(() => {
    rollFrame = null;
    const top = Math.max(0, toolbar.getBoundingClientRect().bottom - 8);
    const topRoll = Math.min(1, Math.max(0, window.scrollY / 80));
    for (const row of visibleInboxRows) {
      const bounds = row.getBoundingClientRect();
      const center = bounds.top + bounds.height / 2;
      const topProximity = Math.max(0, Math.min(1, (center - top) / 220));
      const proximity = Math.max(
        0,
        Math.min(1, 1 - topRoll * (1 - topProximity), (innerHeight - center) / 220),
      );
      row.style.setProperty('--inbox-roll-scale', String(0.96 + 0.04 * proximity));
    }
  });
}
async function loadMore() {
  if (!inboxNext || morePending) return;
  const cursor = inboxNext,
    version = generation;
  morePending = true;
  const trigger = content.querySelector('[data-inbox-more]');
  if (trigger) {
    trigger.disabled = true;
    trigger.textContent = 'Loading more…';
  }
  try {
    const data = await api(
      `inbox?limit=${inboxPageSize}&before=${encodeURIComponent(cursor)}`,
    );
    if (version !== generation) return;
    const known = new Set(entries.map((entry) => entry.task.id));
    for (const entry of data.entries) {
      if (known.has(entry.task.id)) continue;
      known.add(entry.task.id);
      entries.push(entry);
      content.insertBefore(taskCard(entry, true), trigger);
    }
    inboxNext = data.next;
    inboxRevision++;
    trigger?.remove();
    decorateInbox();
    morePending = false;
    addInboxMore();
    requestAnimationFrame(updateInboxFade);
  } catch (error) {
    if (version === generation && trigger) {
      moreObserver?.disconnect();
      trigger.disabled = false;
      trigger.textContent = 'Could not load more. Try again';
      fail(error);
    }
  } finally {
    if (version === generation) morePending = false;
    else if (trigger?.isConnected) {
      trigger.disabled = false;
      trigger.textContent = 'Load more tasks';
      moreObserver?.unobserve(trigger);
      moreObserver?.observe(trigger);
    }
  }
}
function renderInbox() {
  if (!toolbar.querySelector('.inbox-capture')) {
    const trigger = node('a', 'Capture a task…', 'inbox-capture');
    trigger.href = '/capture?list=' + encodeURIComponent(inboxId);
    trigger.dataset.quickCapture = '';
    trigger.dataset.captureList = inboxId;
    toolbar.append(trigger);
  }
  moreObserver?.disconnect();
  rollObserver?.disconnect();
  visibleInboxRows.clear();
  content.replaceChildren(...entries.map((entry) => taskCard(entry, true)));
  decorateInbox();
  addInboxMore();
  if (!entries.length && !inboxNext) {
    const empty = node('div', null, 'planning-empty');
    empty.innerHTML =
      '<svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#inbox"/></svg>';
    empty.append(
      node('h2', 'Inbox zero.'),
      node(
        'p',
        'Everything here is settled. Capture your next thought whenever you’re ready.',
      ),
    );
    content.append(empty);
  }
  requestAnimationFrame(updateInboxFade);
}
function addInboxMore() {
  moreObserver?.disconnect();
  if (inboxNext) {
    const more = button('Load more tasks', loadMore, 'inbox-load-more');
    more.dataset.inboxMore = '';
    content.append(more);
    if ('IntersectionObserver' in window) {
      moreObserver = new IntersectionObserver(
        (records) => {
          if (records.some((record) => record.isIntersecting)) loadMore();
        },
        { rootMargin: '240px 0px' },
      );
      moreObserver.observe(more);
    }
  }
}
function addPlannedTask(date, initialTime = '') {
  const d = modal('Add to ' + labelDate(date));
  const form = node('form'),
    title = node('input'),
    time = node('input'),
    priority = node('select'),
    description = node('textarea');
  title.placeholder = 'What would you like to do?';
  title.required = true;
  title.maxLength = 500;
  title.setAttribute('aria-label', 'Task title');
  time.type = 'time';
  time.value = initialTime;
  time.setAttribute('aria-label', 'Time (optional)');
  const timeLabel = node('label', 'Time (optional)');
  timeLabel.append(time);
  const priorityLabel = node('label', 'Priority');
  for (const [value, label] of [
    [0, 'P0 · Urgent'],
    [1, 'P1 · High'],
    [2, 'P2 · Medium'],
    [3, 'P3 · Low'],
  ]) {
    const option = node('option', label);
    option.value = value;
    priority.append(option);
  }
  priority.value = '3';
  priority.setAttribute('aria-label', 'Priority');
  priorityLabel.append(priority);
  const fields = node('div', null, 'workbench-date-fields');
  fields.append(timeLabel, priorityLabel);
  const descriptionLabel = node('label', 'Description (optional)');
  description.rows = 2;
  description.maxLength = 100000;
  description.placeholder = 'Add a little context…';
  description.setAttribute('aria-label', 'Description (optional)');
  description.className = 'workbench-description';
  description.addEventListener('input', () => {
    description.style.height = 'auto';
    description.style.height = description.scrollHeight + 'px';
  });
  descriptionLabel.append(description);
  const submit = node('button', 'Add task', 'primary');
  submit.type = 'submit';
  form.append(title, fields, descriptionLabel, submit);
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
            tasks: [{ title: title.value.trim(), description: description.value.trim() }],
          },
          key,
        );
        captured = rows[0];
        title.disabled = true;
        description.disabled = true;
      }
      await api(`tasks/${encodeURIComponent(captured.id)}/placement`, 'PUT', {
        revision: captured.revision,
        date,
        time: time.value || null,
        timezone: zone,
        priority: Number(priority.value),
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
  if (pendingSettles.size) {
    deferredRefresh = true;
    return;
  }
  const version = ++generation;
  const inboxSnapshot = inboxRevision;
  morePending = false;
  const selection =
    main.dataset.workbench === 'inbox' ? 'inbox' : `${view}:${anchor}:${query}`;
  const sameSelection = content.dataset.selection === selection;
  const skeleton = beginLoading(content, {
    variant: main.dataset.workbench === 'inbox' ? 'rows' : 'calendar',
    preserve: sameSelection && content.dataset.loaded === 'true',
  });
  if (!sameSelection) content.dataset.loaded = 'false';
  content.dataset.selection = selection;
  if (!sameSelection || content.dataset.loaded !== 'true') content.inert = true;
  content.setAttribute('aria-busy', 'true');
  status.textContent = '';
  try {
    const intake = main.dataset.workbench === 'inbox';
    if (!intake) range();
    let data;
    if (intake) {
      const target = Math.max(inboxPageSize, entries.length);
      data = await api(`inbox?limit=${inboxPageSize}`);
      while (data.next && data.entries.length < target) {
        const page = await api(
          `inbox?limit=${inboxPageSize}&before=${encodeURIComponent(data.next)}`,
        );
        data.entries.push(...page.entries);
        data.next = page.next;
        if (version !== generation) return;
      }
    } else data = await api(`plan?from=${from}&to=${to}`);
    if (version !== generation) return;
    if (intake && inboxSnapshot !== inboxRevision) {
      deferredRefresh = true;
      setTimeout(resumeRefresh, 0);
      return;
    }
    if (sameSelection && content.dataset.loaded === 'true') {
      if (
        JSON.stringify(entries) === JSON.stringify(data.entries) &&
        (!intake || inboxNext === data.next)
      )
        return;
      if (
        pointerHeld ||
        pendingSettles.size ||
        main.querySelector('input:focus,textarea:focus,select:focus')
      ) {
        deferredRefresh = true;
        return;
      }
    }
    entries = data.entries;
    if (intake) inboxNext = data.next;
    inboxId = data.list_id;
    status.textContent = '';
    if (intake) renderInbox();
    else renderPlan();
    content.dataset.loaded = 'true';
  } catch (e) {
    if (version === generation) {
      skeleton.fail('Could not load tasks.', load);
      fail(e);
      status.textContent = 'Could not refresh. ';
      status.append(button('Try again', load));
    }
  } finally {
    skeleton.finish();
    if (version === generation) {
      content.inert = false;
      content.removeAttribute('aria-busy');
    }
  }
}
if (main) {
  main.dataset.planningToday = today;
  content.dataset.planningRows = '';
  await import('./planning-page.js').catch(fail);
  document.addEventListener('todoer:workbench-refresh', load);
  main.querySelector('[data-export-workbench]').onclick = () => {
    const scope =
      main.dataset.workbench === 'inbox' ? { kind: 'inbox' } : { kind: 'plan', from, to };
    import('./export-menu.js').then((m) => m.openExport(scope)).catch(fail);
  };
  if (main.dataset.workbench === 'plan') syncRange();
  else {
    if ('IntersectionObserver' in window) {
      rollObserver = new IntersectionObserver(
        (records) => {
          for (const record of records) {
            if (record.isIntersecting) visibleInboxRows.add(record.target);
            else visibleInboxRows.delete(record.target);
          }
          scheduleInboxRoll();
        },
        { rootMargin: '80px 0px' },
      );
    }
    addEventListener('scroll', updateInboxFade, { passive: true });
    addEventListener('resize', updateInboxFade);
  }
  load();
  document.addEventListener('todoer:capture', load);
}
