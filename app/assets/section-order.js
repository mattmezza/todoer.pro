import { pageScope } from './page-lifecycle.js';
// Sections share the list revision, so writes always validate the displayed set.
export function mountSectionOrdering({
  container,
  listId,
  canWrite,
  request,
  changed,
  error,
  blocked,
}) {
  const scope = pageScope();

  if (!canWrite) return;
  let busy = false,
    gesture = null,
    suppressClick = false;
  const sections = () =>
    [...container.querySelectorAll(':scope > .task-section[data-section-id]')].filter(
      (node) => node.dataset.sectionId,
    );
  function afterTarget(target, y) {
    const heading = target.querySelector(':scope > .section-heading');
    const headerBox = heading?.getBoundingClientRect();
    if (headerBox && y <= headerBox.bottom)
      return y > headerBox.top + headerBox.height / 2;
    const box = target.getBoundingClientRect();
    return y > box.top + box.height / 2;
  }
  async function move(source, target, after = false) {
    if (busy || !target || source === target) return;
    if (blocked()) {
      error('Finish or close the active editor before reordering sections.');
      return;
    }
    const nodes = sections();
    const ids = nodes.map((node) => node.dataset.sectionId);
    const from = ids.indexOf(source.dataset.sectionId);
    if (from < 0 || !ids.includes(target.dataset.sectionId)) return;
    ids.splice(from, 1);
    ids.splice(
      ids.indexOf(target.dataset.sectionId) + Number(after),
      0,
      source.dataset.sectionId,
    );
    if (ids.every((id, index) => id === nodes[index].dataset.sectionId)) return;
    busy = true;
    try {
      const list = await request(`/lists/${encodeURIComponent(listId)}`);
      await request(`/lists/${encodeURIComponent(listId)}/sections/reorder`, 'POST', {
        revision: list.revision,
        ids,
      });
      changed();
    } catch (failure) {
      error(`${failure.message} Section order was not changed.`);
    } finally {
      busy = false;
    }
  }
  scope.listen(
    container,
    'click',
    (event) => {
      if (suppressClick) {
        event.preventDefault();
        event.stopImmediatePropagation();
        suppressClick = false;
        return;
      }
      const button = event.target.closest('[data-section-up],[data-section-down]');
      if (!button) return;
      const source = button.closest('.task-section'),
        nodes = sections(),
        index = nodes.indexOf(source);
      const down = button.hasAttribute('data-section-down');
      button.closest('details').open = false;
      move(source, nodes[index + (down ? 1 : -1)], down);
    },
    true,
  );
  scope.listen(container, 'dragstart', (event) => {
    const handle = event.target.closest('[data-section-drag]');
    if (!handle) return;
    gesture = { source: handle.closest('.task-section') };
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData('text/plain', gesture.source.dataset.sectionId);
    gesture.source.dataset.sectionDragging = '';
  });
  scope.listen(container, 'dragover', (event) => {
    if (gesture?.source) event.preventDefault();
  });
  scope.listen(container, 'drop', (event) => {
    if (!gesture?.source) return;
    event.preventDefault();
    const target = event.target.closest('.task-section[data-section-id]');
    if (target?.dataset.sectionId)
      move(gesture.source, target, afterTarget(target, event.clientY));
    gesture.source.removeAttribute('data-section-dragging');
    gesture = null;
  });
  scope.listen(container, 'dragend', () => {
    gesture?.source?.removeAttribute('data-section-dragging');
    gesture = null;
  });
  scope.listen(
    container,
    'touchstart',
    (event) => {
      const handle = matchMedia('(max-width: 640px)').matches
        ? !event.target.closest('.entity-menu, .entity-reference') &&
          event.target.closest('.section-heading')
        : event.target.closest('[data-section-drag]');
      if (!handle || event.touches.length !== 1) return;
      if (!handle.closest('.task-section')?.dataset.sectionId) return;
      const touch = event.touches[0];
      gesture = {
        source: handle.closest('.task-section'),
        x: touch.clientX,
        y: touch.clientY,
        ready: false,
      };
      const state = gesture;
      state.timer = scope.timeout(() => {
        if (gesture === state) {
          state.ready = true;
          state.source.dataset.sectionDragging = '';
        }
      }, 350);
    },
    { passive: true },
  );
  scope.listen(
    container,
    'touchmove',
    (event) => {
      if (!gesture?.timer || event.touches.length !== 1) return;
      const touch = event.touches[0];
      if (!gesture.ready) {
        if (Math.hypot(touch.clientX - gesture.x, touch.clientY - gesture.y) > 10) {
          clearTimeout(gesture.timer);
          gesture = null;
        }
        return;
      }
      event.preventDefault();
      if (touch.clientY < 70) window.scrollBy(0, -12);
      else if (touch.clientY > window.innerHeight - 90) window.scrollBy(0, 12);
      const target = document
        .elementFromPoint(touch.clientX, touch.clientY)
        ?.closest('.task-section[data-section-id]');
      for (const node of sections()) node.removeAttribute('data-section-drop');
      if (target?.dataset.sectionId && target !== gesture.source) {
        gesture.target = target;
        gesture.after = afterTarget(target, touch.clientY);
        target.dataset.sectionDrop = gesture.after ? 'after' : 'before';
      } else gesture.target = null;
    },
    { passive: false },
  );
  function endTouch() {
    if (!gesture?.timer) return;
    clearTimeout(gesture.timer);
    if (gesture.ready) {
      suppressClick = true;
      scope.timeout(() => {
        suppressClick = false;
      }, 500);
      move(gesture.source, gesture.target, gesture.after);
    }
    gesture.source.removeAttribute('data-section-dragging');
    for (const node of sections()) node.removeAttribute('data-section-drop');
    gesture = null;
  }
  scope.listen(container, 'touchend', endTouch);
  scope.listen(container, 'touchcancel', () => {
    if (gesture) gesture.target = null;
    endTouch();
  });
}
