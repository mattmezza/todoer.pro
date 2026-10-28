import { request } from './request.js';
const bell = document.querySelector('[data-notification-bell]');
const center = document.querySelector('[data-notification-center]');
if (bell) {
  // Measure visible right docks rather than duplicating their responsive widths.
  let dockFrame;
  const observedDocks = new Set();
  const resizeDock = new ResizeObserver(() => queueDock());
  function positionBell() {
    dockFrame = null;
    let edge = innerWidth;
    const panels = [
      ...document.querySelectorAll(
        '.planning-detail-panel, .list-settings-panel.is-docked[open]',
      ),
    ];
    for (const panel of observedDocks) {
      if (!panels.includes(panel)) {
        resizeDock.unobserve(panel);
        observedDocks.delete(panel);
      }
    }
    panels.forEach((panel) => {
      if (!observedDocks.has(panel)) {
        resizeDock.observe(panel);
        observedDocks.add(panel);
      }
      const box = panel.getBoundingClientRect();
      if (box.width && box.height && getComputedStyle(panel).visibility !== 'hidden') {
        edge = Math.min(
          edge,
          innerWidth -
            box.width -
            Math.max(0, parseFloat(getComputedStyle(panel).right) || 0),
        );
      }
    });
    document.body.style.setProperty(
      '--notification-panel-offset',
      `${Math.max(0, innerWidth - edge)}px`,
    );
    bell.hidden = edge < 90;
  }
  function queueDock() {
    if (!dockFrame) dockFrame = requestAnimationFrame(positionBell);
  }
  new MutationObserver((records) => {
    if (
      records.some(
        (record) =>
          record.type === 'childList' ||
          record.target === document.body ||
          record.target.matches?.('.planning-detail-panel, .list-settings-panel'),
      )
    )
      queueDock();
  }).observe(document.body, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ['class', 'open', 'hidden'],
  });
  window.addEventListener('resize', queueDock);
  queueDock();

  const toggle = bell.querySelector('[data-notification-toggle]');
  const popover = bell.querySelector('.notification-popover');
  const badge = bell.querySelector('[data-notification-badge]');
  let latest = null,
    through = 0,
    cursor = null,
    unreadOnly = false,
    lastUnread = null,
    pendingReload = false;
  let polling = false,
    loading = false,
    timer,
    failed = false;
  const csrf = document.querySelector('meta[name="csrf-token"]')?.content;
  const icon = (name) => {
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.classList.add('ui-icon');
    svg.setAttribute('aria-hidden', 'true');
    const use = document.createElementNS(svg.namespaceURI, 'use');
    use.setAttribute('href', `/assets/icons.svg#${name}`);
    svg.append(use);
    return svg;
  };
  function error(root, message) {
    const el = root?.querySelector('[data-notification-error]');
    if (el) {
      el.textContent = message;
      el.hidden = !message;
    }
  }
  async function api(path, body) {
    const r = await request(path, {
      cache: 'no-store',
      credentials: 'same-origin',
      method: body ? 'POST' : 'GET',
      headers: body ? { 'Content-Type': 'application/json', 'x-csrf-token': csrf } : {},
      body: body ? JSON.stringify(body) : undefined,
    });
    if (!r.ok) throw Error('Could not update notifications. Please try again.');
    return r.status === 204 ? null : r.json();
  }
  const labels = {
    due: 'Task overdue',
    reminder: 'Reminder',
    update: 'Shared task updated',
  };
  function render(root, items, append = false) {
    if (!append) root.replaceChildren();
    if (!items.length && !append) {
      const empty = document.createElement('div');
      empty.className = 'notification-empty';
      const p = document.createElement('p');
      p.textContent =
        unreadOnly && root.matches('[data-notification-items]')
          ? 'You’re all caught up.'
          : 'No notifications yet.';
      const small = document.createElement('small');
      small.textContent = 'Your reminders and shared updates will appear here.';
      empty.append(icon('bell'), p, small);
      root.append(empty);
    }
    for (const n of items) {
      const item = document.createElement('article');
      item.className = `notification-item${n.unread ? ' unread' : ''}`;
      const link = document.createElement('a');
      link.href = `/tasks/${encodeURIComponent(n.task_reference.replace(/^%/, ''))}`;
      const text = document.createElement('span');
      text.className = 'notification-text';
      const kind = document.createElement('small');
      kind.textContent =
        n.kind === 'update' && n.actor_name
          ? `Updated by ${n.actor_name}`
          : labels[n.kind] || 'Task update';
      const title = document.createElement('strong');
      title.textContent = n.title;
      const meta = document.createElement('span');
      meta.className = 'notification-meta';
      const time = document.createElement('time');
      const date = new Date(n.created_at * 1000);
      time.dateTime = date.toISOString();
      time.textContent = date.toLocaleString(undefined, {
        month: 'short',
        day: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
      });
      meta.append(document.createTextNode(n.list_title + ' · '), time);
      text.append(kind, title, meta);
      link.append(
        icon(n.kind === 'due' ? 'clock' : n.kind === 'reminder' ? 'bell' : 'edit'),
        text,
      );
      // Navigation does not wait on the network. A failed acknowledgement keeps
      // the notice unread on the next visit rather than silently losing it.
      link.addEventListener('click', () => {
        globalThis
          .fetch('/api/v1/notifications/read', {
            method: 'POST',
            credentials: 'same-origin',
            keepalive: true,
            headers: { 'Content-Type': 'application/json', 'x-csrf-token': csrf },
            body: JSON.stringify({ through: n.id, id: n.id }),
          })
          .catch(() => {});
      });
      item.append(link);
      if (n.unread) {
        const read = document.createElement('button');
        read.type = 'button';
        read.className = 'notification-read';
        read.title = 'Mark as read';
        read.setAttribute('aria-label', `Mark ${n.title} as read`);
        read.append(icon('check'));
        read.addEventListener('click', async () => {
          read.disabled = true;
          try {
            await api('/api/v1/notifications/read', { through: n.id, id: n.id });
            await refresh();
            if (center) await loadCenter();
          } catch (e) {
            error(center || bell, e.message);
          } finally {
            read.disabled = false;
          }
        });
        item.append(read);
      }
      root.append(item);
    }
  }
  async function refresh() {
    if (polling || document.hidden || !navigator.onLine) return;
    polling = true;
    try {
      const data = await api('/api/v1/notifications?limit=5');
      through = data.latest_id;
      const fresh = latest !== null && data.latest_id > latest;
      const changed = lastUnread !== null && data.unread !== lastUnread;
      lastUnread = data.unread;
      // Progressive enhancement: keep the installed app's icon in sync too.
      if ('setAppBadge' in navigator) {
        const update = data.unread
          ? navigator.setAppBadge(data.unread)
          : navigator.clearAppBadge();
        update.catch(() => {});
      }
      latest = data.latest_id;
      badge.hidden = data.unread === 0;
      badge.textContent = data.unread > 99 ? '99+' : String(data.unread);
      toggle.classList.toggle('has-unread', data.unread > 0);
      toggle.setAttribute(
        'aria-label',
        data.unread ? `Notifications, ${data.unread} unread` : 'Notifications',
      );
      if (fresh && data.unread > 0) {
        toggle.classList.remove('notification-arrived');
        void toggle.offsetWidth;
        toggle.classList.add('notification-arrived');
        bell.querySelector('[data-notification-announcement]').textContent =
          `New notification. ${data.unread} unread.`;
      }
      if (center && (fresh || changed)) await loadCenter();
      render(bell.querySelector('[data-notification-recent]'), data.items);
      bell.querySelector('[data-notification-read-all]').disabled = !data.unread;
      if (center)
        center.querySelector('[data-notification-read-all]').disabled = !data.unread;
      error(bell, '');
      failed = false;
    } catch (e) {
      failed = true;
      error(bell, 'Notifications are unavailable. Reopen to retry.');
    } finally {
      polling = false;
      clearTimeout(timer);
      timer = setTimeout(refresh, failed ? 30000 : 15000);
    }
  }
  async function loadCenter(append = false) {
    if (!center) return;
    if (loading) {
      if (!append) pendingReload = true;
      return;
    }
    loading = true;
    const more = center.querySelector('[data-notification-more]');
    more.disabled = true;
    try {
      const data = await api(
        `/api/v1/notifications?limit=25&unread=${unreadOnly}${append && cursor ? `&before=${cursor}` : ''}`,
      );
      render(center.querySelector('[data-notification-items]'), data.items, append);
      cursor = data.next_cursor;
      more.hidden = !cursor;
      error(center, '');
    } catch (e) {
      error(center, e.message);
    } finally {
      loading = false;
      more.disabled = false;
      if (pendingReload) {
        pendingReload = false;
        loadCenter();
      }
    }
  }
  function open(value) {
    popover.hidden = !value;
    toggle.setAttribute('aria-expanded', String(value));
    if (value) refresh();
  }
  toggle.addEventListener('click', () => open(popover.hidden));
  document.addEventListener('click', (e) => {
    if (!bell.contains(e.target)) open(false);
  });
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && !popover.hidden) {
      open(false);
      toggle.focus();
    }
  });
  for (const root of [bell, center].filter(Boolean))
    root
      .querySelector('[data-notification-read-all]')
      .addEventListener('click', async (e) => {
        e.currentTarget.disabled = true;
        try {
          await api('/api/v1/notifications/read', { through });
          await refresh();
          if (center) await loadCenter();
        } catch (err) {
          error(root, err.message);
          e.target.disabled = false;
        }
      });
  center
    ?.querySelector('[data-notification-more]')
    .addEventListener('click', () => loadCenter(true));
  center?.querySelectorAll('[data-notification-filter]').forEach((button) =>
    button.addEventListener('click', () => {
      if (loading) return;
      unreadOnly = button.dataset.notificationFilter === 'unread';
      center.querySelectorAll('[data-notification-filter]').forEach((b) => {
        b.classList.toggle('active', b === button);
        b.setAttribute('aria-pressed', String(b === button));
      });
      loadCenter();
    }),
  );
  document.addEventListener('visibilitychange', () => {
    if (!document.hidden) refresh();
  });
  window.addEventListener('online', refresh);
  refresh();
  if (center) loadCenter();
}
