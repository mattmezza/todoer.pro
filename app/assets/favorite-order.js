import { request } from './request.js';
import { toast } from './toast.js';

const selector = '[data-favorite-order]';
let drag = null,
  busy = false,
  suppressClick = false;
const entries = (nav) => [...nav.querySelectorAll(':scope > [data-favorite-id]')];
const key = (item) => ({ kind: item.dataset.favoriteKind, id: item.dataset.favoriteId });
async function reorder(source, target, after) {
  if (busy || !target || source === target) return;
  const nav = source.closest(selector);
  if (!nav || target.closest(selector) !== nav) return;
  const original = entries(nav),
    expected = original.map(key);
  target.before(source);
  if (after) target.after(source);
  const order = entries(nav).map(key);
  if (JSON.stringify(expected) === JSON.stringify(order)) return;
  busy = true;
  try {
    const response = await request('/api/v1/favorites/order', {
      method: 'PUT',
      headers: {
        'Content-Type': 'application/json',
        'X-CSRF-Token': document.querySelector('meta[name="csrf-token"]')?.content || '',
      },
      body: JSON.stringify({ expected, order }),
    });
    if (!response.ok)
      throw Error(
        (await response.json()).error?.message || 'Could not reorder favorites.',
      );
  } catch (error) {
    if (nav.isConnected) nav.replaceChildren(...original);
    toast(error.message);
  } finally {
    busy = false;
  }
}
function clear() {
  document
    .querySelectorAll('[data-favorite-dragging],[data-favorite-drop]')
    .forEach((item) => {
      item.removeAttribute('data-favorite-dragging');
      item.removeAttribute('data-favorite-drop');
    });
}
document.addEventListener('dragstart', (event) => {
  const source = event.target.closest(`${selector} > [data-favorite-id]`);
  if (!source || busy) return;
  drag = { source };
  source.dataset.favoriteDragging = '';
  event.dataTransfer.effectAllowed = 'move';
  event.dataTransfer.setData('text/plain', source.dataset.favoriteId);
});
document.addEventListener('dragover', (event) => {
  if (!drag?.source || !event.target.closest(selector)) return;
  event.preventDefault();
  const target = event.target.closest('[data-favorite-id]');
  clear();
  drag.source.dataset.favoriteDragging = '';
  if (target && target !== drag.source)
    target.dataset.favoriteDrop =
      event.clientY > target.getBoundingClientRect().top + target.offsetHeight / 2
        ? 'after'
        : 'before';
});
document.addEventListener('drop', (event) => {
  if (!drag?.source) return;
  event.preventDefault();
  const target = event.target.closest(`${selector} > [data-favorite-id]`);
  if (target)
    reorder(
      drag.source,
      target,
      event.clientY > target.getBoundingClientRect().top + target.offsetHeight / 2,
    );
  clear();
  drag = null;
});
document.addEventListener('dragend', () => {
  clear();
  drag = null;
});
document.addEventListener('keydown', (event) => {
  const source = event.target.closest(`${selector} > [data-favorite-id]`);
  if (!source || !event.altKey || !['ArrowUp', 'ArrowDown'].includes(event.key)) return;
  event.preventDefault();
  const items = entries(source.parentElement),
    down = event.key === 'ArrowDown';
  reorder(source, items[items.indexOf(source) + (down ? 1 : -1)], down);
});
document.addEventListener(
  'click',
  (event) => {
    if (suppressClick && event.target.closest(selector)) {
      event.preventDefault();
      event.stopImmediatePropagation();
      suppressClick = false;
    }
  },
  true,
);
document.addEventListener(
  'touchstart',
  (event) => {
    const source = event.target.closest(`${selector} > [data-favorite-id]`);
    if (!source || event.touches.length !== 1 || busy) return;
    const touch = event.touches[0];
    drag = { source, x: touch.clientX, y: touch.clientY, ready: false };
    const state = drag;
    state.timer = setTimeout(() => {
      if (drag === state) {
        state.ready = true;
        source.dataset.favoriteDragging = '';
      }
    }, 400);
  },
  { passive: true },
);
document.addEventListener(
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
    const scroller = drag.source.closest('.sidebar-scroll');
    if (scroller) {
      const bounds = scroller.getBoundingClientRect();
      if (touch.clientY < bounds.top + 50) scroller.scrollBy(0, -12);
      else if (touch.clientY > bounds.bottom - 50) scroller.scrollBy(0, 12);
    }
    const target = document
      .elementFromPoint(touch.clientX, touch.clientY)
      ?.closest(`${selector} > [data-favorite-id]`);
    clear();
    drag.source.dataset.favoriteDragging = '';
    drag.target = target && target !== drag.source ? target : null;
    if (drag.target) {
      drag.after =
        touch.clientY > target.getBoundingClientRect().top + target.offsetHeight / 2;
      target.dataset.favoriteDrop = drag.after ? 'after' : 'before';
    }
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
    if (!cancel) reorder(drag.source, drag.target, drag.after);
  }
  clear();
  drag = null;
}
document.addEventListener('touchend', () => endTouch());
document.addEventListener('touchcancel', () => endTouch(true));
