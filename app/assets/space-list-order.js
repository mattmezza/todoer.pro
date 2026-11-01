import { request } from './request.js';
import { toast } from './toast.js';

const grid = document.querySelector('[data-list-order-enabled]');
const root = grid?.closest('[data-space-id]');
const status = document.querySelector('[data-list-order-status]');
let drag = null,
  busy = false,
  suppressClick = false;
const cards = () => [...grid.querySelectorAll(':scope > [data-list-order-id]')];
function clear() {
  grid.querySelectorAll('[data-list-dragging],[data-list-drop]').forEach((card) => {
    card.removeAttribute('data-list-dragging');
    card.removeAttribute('data-list-drop');
  });
}
function targetAt(x, y) {
  return document.elementFromPoint(x, y)?.closest('[data-list-order-id]');
}
function afterTarget(target, x, y) {
  const rect = target.getBoundingClientRect();
  return rect.width > grid.getBoundingClientRect().width * 0.75
    ? y > rect.top + rect.height / 2
    : x > rect.left + rect.width / 2;
}
function markTarget(target, x, y) {
  clear();
  drag.source.dataset.listDragging = '';
  drag.target =
    target && target.parentElement === grid && target !== drag.source ? target : null;
  if (drag.target) {
    drag.after = afterTarget(target, x, y);
    target.dataset.listDrop = drag.after ? 'after' : 'before';
  }
}
async function move(source, target, after) {
  if (busy || !target || source === target) return;
  const original = cards();
  if (after) target.after(source);
  else target.before(source);
  const ordered = cards();
  if (ordered.every((card, index) => card === original[index])) return;
  busy = true;
  try {
    const response = await request(
      `/api/v1/workspaces/${encodeURIComponent(root.dataset.spaceId)}/lists/reorder-visible`,
      {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'X-CSRF-Token': document.querySelector('meta[name="csrf-token"]').content,
        },
        body: JSON.stringify({
          revision: Number(root.dataset.spaceRevision),
          ids: ordered.map((card) => card.dataset.listOrderId),
        }),
      },
    );
    if (!response.ok)
      throw Error(
        (await response.json().catch(() => null))?.error?.message ||
          'Could not reorder lists.',
      );
    const workspace = await response.json();
    root.dataset.spaceRevision = workspace.revision;
    status.textContent = 'List order saved.';
  } catch (error) {
    grid.replaceChildren(...original);
    toast(`${error.message} Refresh to check the saved order.`);
    status.textContent = 'List order could not be confirmed.';
  } finally {
    busy = false;
  }
}
if (grid) {
  grid.addEventListener('dragstart', (event) => {
    const source = event.target.closest('[data-list-order-id]');
    if (!source || busy) {
      event.preventDefault();
      return;
    }
    drag = { source };
    source.dataset.listDragging = '';
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData('text/plain', source.dataset.listOrderId);
  });
  grid.addEventListener('dragover', (event) => {
    if (!drag?.source) return;
    event.preventDefault();
    markTarget(
      event.target.closest('[data-list-order-id]'),
      event.clientX,
      event.clientY,
    );
  });
  grid.addEventListener('drop', (event) => {
    if (!drag?.source) return;
    event.preventDefault();
    move(drag.source, drag.target, drag.after);
    clear();
    drag = null;
  });
  grid.addEventListener('dragend', () => {
    clear();
    drag = null;
  });
  grid.addEventListener('keydown', (event) => {
    const source = event.target.closest('[data-list-order-id]');
    if (
      !source ||
      !event.altKey ||
      !['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(event.key)
    )
      return;
    event.preventDefault();
    const items = cards(),
      forward = ['ArrowDown', 'ArrowRight'].includes(event.key);
    move(source, items[items.indexOf(source) + (forward ? 1 : -1)], forward);
  });
  grid.addEventListener(
    'click',
    (event) => {
      if (suppressClick) {
        event.preventDefault();
        event.stopImmediatePropagation();
        suppressClick = false;
      }
    },
    true,
  );
  grid.addEventListener(
    'touchstart',
    (event) => {
      const source = event.target.closest('[data-list-order-id]');
      if (!source || event.touches.length !== 1 || busy) return;
      const touch = event.touches[0];
      drag = { source, x: touch.clientX, y: touch.clientY, ready: false };
      const state = drag;
      state.timer = setTimeout(() => {
        if (drag === state) {
          state.ready = true;
          source.dataset.listDragging = '';
        }
      }, 400);
    },
    { passive: true },
  );
  grid.addEventListener(
    'touchmove',
    (event) => {
      if (!drag?.timer || event.touches.length !== 1) return;
      const touch = event.touches[0];
      if (!drag.ready) {
        if (Math.hypot(touch.clientX - drag.x, touch.clientY - drag.y) > 10) {
          clearTimeout(drag.timer);
          drag = null;
        }
        return;
      }
      event.preventDefault();
      if (touch.clientY < 60) window.scrollBy(0, -14);
      else if (touch.clientY > innerHeight - 80) window.scrollBy(0, 14);
      markTarget(targetAt(touch.clientX, touch.clientY), touch.clientX, touch.clientY);
    },
    { passive: false },
  );
  function endTouch(cancel = false) {
    if (!drag?.timer) return;
    clearTimeout(drag.timer);
    if (drag.ready) {
      suppressClick = true;
      setTimeout(() => {
        suppressClick = false;
      }, 500);
      if (!cancel) move(drag.source, drag.target, drag.after);
    }
    clear();
    drag = null;
  }
  grid.addEventListener('touchend', () => endTouch());
  grid.addEventListener('touchcancel', () => endTouch(true));
}
