// Ordering never moves DOM nodes optimistically: a confirmed write refreshes the
// canonical fragment, and a rejected write leaves drafts and selections intact.
// Native HTML5 drag is enabled only on desktop-width fine-pointer/hover devices.
// Phones and coarse-pointer devices use the same 44px Up/Down actions instead.
export function mountTaskOrdering({
  container,
  listId,
  canWrite,
  request,
  onChanged,
  onError,
  hasActiveDraft,
  onSwipe,
}) {
  if (!container || !listId || typeof request !== 'function')
    throw new Error('Ordering requires a list, container and request function');
  const desktop = matchMedia('(min-width: 640px) and (hover: hover) and (pointer: fine)');
  let active = true,
    busy = false,
    drag = null;
  const controls = () => [
    ...container.querySelectorAll(
      '[data-order-up],[data-order-down],[data-order-handle]',
    ),
  ];
  function configure() {
    for (const control of controls()) {
      control.disabled = !canWrite || busy;
      if (!canWrite) control.hidden = true;
      if (control.hasAttribute('data-order-handle')) {
        control.hidden = !canWrite;
        control.draggable = canWrite && desktop.matches && !busy;
      }
    }
  }
  function error(message) {
    if (active) onError?.(message, true);
  }
  function blocked() {
    if (hasActiveDraft?.()) {
      error(
        'Finish or close the active editor, clear the capture draft or text selection, then reorder. Your draft is unchanged.',
      );
      return true;
    }
    return false;
  }
  function clearMarkers() {
    for (const row of container.querySelectorAll(
      '[data-order-dragging],[data-order-drop]',
    )) {
      row.removeAttribute('data-order-dragging');
      row.removeAttribute('data-order-drop');
    }
  }
  async function snapshot() {
    // The revision protects order writes; server validation protects the exact
    // active sibling set. Completed tasks are deliberately retained here.
    const [info, tasks] = await Promise.all([
      request(`/lists/${encodeURIComponent(listId)}`),
      request(`/lists/${encodeURIComponent(listId)}/tasks`),
    ]);
    if (!Number.isInteger(info.revision) || !Array.isArray(tasks))
      throw new Error('Could not read the current sibling order. Reload and try again.');
    return { revision: info.revision, tasks };
  }
  function siblings(state, source, row) {
    const task = state.tasks.find((t) => t.id === source);
    if (!task || task.list_id !== listId)
      throw new Error(
        'The task moved or is no longer available. Reload before reordering.',
      );
    if (
      row?.hasAttribute('data-parent-id') &&
      row.dataset.parentId !== (task.parent_id || '')
    )
      throw new Error('The task changed parent. Reload before reordering.');
    const ids = state.tasks
      .filter((t) => t.list_id === listId && t.parent_id === task.parent_id)
      .map((t) => t.id);
    return { task, ids };
  }
  async function commit(state, task, ids) {
    if (!active || blocked()) return;
    await request(`/lists/${encodeURIComponent(listId)}/task-order`, 'POST', {
      parent_id: task.parent_id,
      revision: state.revision,
      ids,
    });
    if (active) onChanged?.();
  }
  async function run(action) {
    if (!active || busy || !canWrite || blocked()) return;
    busy = true;
    configure();
    try {
      await action();
    } catch (e) {
      if (!active) return;
      error(
        `${e.message || 'Could not reorder tasks'}. The displayed order was not moved. Your drafts and selection are unchanged.`,
      );
      // Parent refresh deliberately respects any editor opened during the request.
      if (e.status === 409 || e.status === 403 || e.status === 404) onChanged?.();
    } finally {
      busy = false;
      clearMarkers();
      if (active) configure();
    }
  }
  function click(event) {
    const control = event.target.closest('[data-order-up],[data-order-down]');
    if (!control || !container.contains(control)) return;
    event.preventDefault();
    event.stopPropagation();
    // An action menu is not an edit draft. Closing only this menu also permits
    // the parent to perform its normal draft-preserving canonical refresh.
    const menu = control.closest('details');
    if (menu) menu.open = false;
    const row = control.closest('.task[data-id]');
    if (!row) return;
    run(async () => {
      const state = await snapshot();
      if (!active || blocked()) return;
      const { task, ids } = siblings(state, row.dataset.id, row);
      const from = ids.indexOf(task.id),
        to = from + (control.hasAttribute('data-order-up') ? -1 : 1);
      if (to < 0 || to >= ids.length) {
        error(
          'This task is already at that end of its full sibling order, including completed tasks.',
        );
        return;
      }
      [ids[from], ids[to]] = [ids[to], ids[from]];
      await commit(state, task, ids);
    });
  }
  function dragStart(event) {
    const handle = event.target.closest('[data-order-handle]');
    if (!handle) return;
    if (
      !container.contains(handle) ||
      !canWrite ||
      busy ||
      !desktop.matches ||
      blocked()
    ) {
      event.preventDefault();
      return;
    }
    const row = handle.closest('.task[data-id]');
    if (!row) {
      event.preventDefault();
      return;
    }
    // Keep the handle visible for the native drag. Hiding its containing menu
    // during dragstart cancels native dragging in Chromium.
    clearMarkers();
    row.dataset.orderDragging = 'true';
    const pending = snapshot();
    // Attach a rejection handler immediately; a cancelled native drag may never
    // drop and therefore may never await this read.
    pending.catch(() => {});
    drag = { source: row.dataset.id, row, pending, menu: handle.closest('details') };
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData('text/plain', row.dataset.id);
    event.dataTransfer.setDragImage(row, 24, 24);
  }
  function dragOver(event) {
    if (!drag || !canWrite || !desktop.matches || busy) return;
    const row = event.target.closest('.task[data-id]');
    if (!row || !container.contains(row) || row.dataset.id === drag.source) return;
    if (
      row.hasAttribute('data-parent-id') &&
      drag.row.hasAttribute('data-parent-id') &&
      row.dataset.parentId !== drag.row.dataset.parentId
    )
      return;
    event.preventDefault();
    event.dataTransfer.dropEffect = 'move';
    for (const old of container.querySelectorAll('[data-order-drop]'))
      old.removeAttribute('data-order-drop');
    const box = row.getBoundingClientRect();
    row.dataset.orderDrop = event.clientY < box.top + box.height / 2 ? 'before' : 'after';
  }
  function drop(event) {
    const current = drag;
    const row = event.target.closest('.task[data-id]');
    if (!current || !row || !container.contains(row)) return;
    event.preventDefault();
    event.stopPropagation();
    drag = null;
    if (current.menu) current.menu.open = false;
    const box = row.getBoundingClientRect();
    const after = event.clientY >= box.top + box.height / 2;
    run(async () => {
      const state = await current.pending;
      if (!active || blocked()) return;
      const { task, ids } = siblings(state, current.source, current.row);
      const target = state.tasks.find((t) => t.id === row.dataset.id);
      if (!target || target.parent_id !== task.parent_id)
        throw new Error('Drop only beside a sibling in the same parent');
      if (target.id === task.id) return;
      const order = ids.filter((id) => id !== task.id);
      const index = order.indexOf(target.id);
      if (index < 0) throw new Error('The drop target moved. Reload before reordering');
      order.splice(index + (after ? 1 : 0), 0, task.id);
      if (order.every((id, index) => id === ids[index])) return;
      await commit(state, task, order);
    });
  }
  function dragEnd() {
    drag = null;
    if (!busy) clearMarkers();
  }
  // Pointer capture supports touch without taking over normal list scrolling.
  // Only the dedicated grip starts a drag; up/down menu actions stay available.
  let pointerDrag = null;
  function pointerDown(event) {
    const handle = event.target.closest('[data-touch-order]');
    if (!handle || !canWrite || busy || blocked()) return;
    const row = handle.closest('.task[data-id]');
    if (!row) return;
    event.preventDefault();
    handle.setPointerCapture(event.pointerId);
    const pending = snapshot();
    pending.catch(() => {});
    pointerDrag = {
      row,
      handle,
      pending,
      target: null,
      after: false,
      startY: event.clientY,
    };
    row.dataset.orderDragging = 'true';
  }
  function pointerMove(event) {
    if (!pointerDrag) return;
    const target = document
      .elementFromPoint(event.clientX, event.clientY)
      ?.closest('.task[data-id]');
    if (
      !target ||
      !container.contains(target) ||
      target === pointerDrag.row ||
      target.dataset.parentId !== pointerDrag.row.dataset.parentId ||
      target.closest('.task-section') !== pointerDrag.row.closest('.task-section')
    )
      return;
    clearMarkers();
    pointerDrag.row.dataset.orderDragging = 'true';
    const rect = target.getBoundingClientRect();
    pointerDrag.after = event.clientY > rect.top + rect.height / 2;
    pointerDrag.target = target;
    target.dataset.orderDrop = pointerDrag.after ? 'after' : 'before';
    if (event.clientY < 80) window.scrollBy(0, -14);
    else if (event.clientY > innerHeight - 100) window.scrollBy(0, 14);
  }
  function pointerUp() {
    const current = pointerDrag;
    pointerDrag = null;
    if (!current) return;
    clearMarkers();
    if (!current.target) return;
    run(async () => {
      const state = await current.pending;
      const { task, ids } = siblings(state, current.row.dataset.id, current.row);
      const order = ids.filter((id) => id !== task.id),
        index = order.indexOf(current.target.dataset.id);
      if (index < 0) throw Error('The destination changed. Reload and try again.');
      order.splice(index + (current.after ? 1 : 0), 0, task.id);
      await commit(state, task, order);
    });
  }
  function pointerCancel() {
    pointerDrag = null;
    clearMarkers();
  }
  // A short horizontal gesture triggers a swipe action. A stationary long press
  // enters reorder mode; ordinary vertical movement remains native scrolling.
  let touchGesture = null;
  let suppressClickUntil = 0;
  function resetTouch() {
    if (!touchGesture) return;
    clearTimeout(touchGesture.timer);
    touchGesture.row.removeAttribute('data-swipe-action');
    touchGesture.row.style.removeProperty('--swipe-distance');
    touchGesture = null;
    if (!busy) clearMarkers();
  }
  function touchStart(event) {
    if (event.touches.length !== 1) {
      resetTouch();
      return;
    }
    if (desktop.matches || !canWrite || busy) return;
    const row = event.target.closest('.task[data-id]');
    if (
      !row ||
      !container.contains(row) ||
      event.target.closest('button,summary,input,textarea,select,details')
    )
      return;
    const point = event.touches[0];
    const gesture = {
      row,
      x: point.clientX,
      y: point.clientY,
      mode: 'pending',
      target: null,
      after: false,
    };
    touchGesture = gesture;
    gesture.timer = setTimeout(() => {
      if (touchGesture !== gesture || blocked()) return;
      gesture.mode = 'drag';
      gesture.pending = snapshot();
      gesture.pending.catch(() => {});
      row.dataset.orderDragging = 'true';
      navigator.vibrate?.(15);
    }, 400);
  }
  function touchMove(event) {
    const current = touchGesture;
    if (!current || event.touches.length !== 1) return;
    const point = event.touches[0],
      dx = point.clientX - current.x,
      dy = point.clientY - current.y;
    if (current.mode === 'pending' && Math.max(Math.abs(dx), Math.abs(dy)) > 10) {
      clearTimeout(current.timer);
      if (Math.abs(dx) > Math.abs(dy) * 1.3) current.mode = 'swipe';
      else {
        resetTouch();
        return;
      }
    }
    if (current.mode === 'swipe') {
      if (event.cancelable) event.preventDefault();
      current.dx = current.row.dataset.complete === 'true' ? Math.min(0, dx) : dx;
      const distance = Math.max(-120, Math.min(120, current.dx));
      current.row.dataset.swipeAction = distance >= 0 ? 'done' : 'trash';
      current.row.style.setProperty('--swipe-distance', `${distance}px`);
    } else if (current.mode === 'drag') {
      if (event.cancelable) event.preventDefault();
      const target = document
        .elementFromPoint(point.clientX, point.clientY)
        ?.closest('.task[data-id]');
      if (
        !target ||
        target === current.row ||
        !container.contains(target) ||
        target.dataset.parentId !== current.row.dataset.parentId ||
        target.closest('.task-section') !== current.row.closest('.task-section')
      )
        return;
      clearMarkers();
      current.row.dataset.orderDragging = 'true';
      const rect = target.getBoundingClientRect();
      current.after = point.clientY > rect.top + rect.height / 2;
      current.target = target;
      target.dataset.orderDrop = current.after ? 'after' : 'before';
      if (point.clientY < 80) window.scrollBy(0, -14);
      else if (point.clientY > innerHeight - 100) window.scrollBy(0, 14);
    }
  }
  function touchEnd() {
    const current = touchGesture;
    if (!current) return;
    if (current.mode !== 'pending') suppressClickUntil = Date.now() + 700;
    resetTouch();
    if (current.mode === 'swipe' && Math.abs(current.dx || 0) >= 80) {
      if (!blocked()) onSwipe?.(current.row, current.dx > 0 ? 'complete' : 'trash');
    } else if (current.mode === 'drag' && current.target) {
      run(async () => {
        const state = await current.pending;
        const { task, ids } = siblings(state, current.row.dataset.id, current.row);
        const order = ids.filter((id) => id !== task.id),
          index = order.indexOf(current.target.dataset.id);
        if (index < 0) throw Error('The destination changed. Reload and try again.');
        order.splice(index + (current.after ? 1 : 0), 0, task.id);
        await commit(state, task, order);
      });
    }
  }
  function suppressTouchClick(event) {
    if (Date.now() < suppressClickUntil) {
      event.preventDefault();
      event.stopImmediatePropagation();
    }
  }
  function suppressTouchMenu(event) {
    if (touchGesture?.mode === 'drag') event.preventDefault();
  }
  container.addEventListener('touchstart', touchStart, { passive: true });
  container.addEventListener('touchmove', touchMove, { passive: false });
  container.addEventListener('touchend', touchEnd);
  container.addEventListener('touchcancel', resetTouch);
  container.addEventListener('click', suppressTouchClick, true);
  container.addEventListener('contextmenu', suppressTouchMenu);
  container.addEventListener('pointerdown', pointerDown);
  container.addEventListener('pointermove', pointerMove);
  container.addEventListener('pointerup', pointerUp);
  container.addEventListener('pointercancel', pointerCancel);
  configure();
  const observer = new MutationObserver(configure);
  observer.observe(container, { childList: true, subtree: true });
  container.addEventListener('click', click);
  container.addEventListener('dragstart', dragStart);
  container.addEventListener('dragover', dragOver);
  container.addEventListener('drop', drop);
  container.addEventListener('dragend', dragEnd);
  desktop.addEventListener('change', configure);
  return () => {
    active = false;
    resetTouch();
    container.removeEventListener('touchstart', touchStart);
    container.removeEventListener('touchmove', touchMove);
    container.removeEventListener('touchend', touchEnd);
    container.removeEventListener('touchcancel', resetTouch);
    container.removeEventListener('click', suppressTouchClick, true);
    container.removeEventListener('contextmenu', suppressTouchMenu);
    drag = null;
    clearMarkers();
    observer.disconnect();
    container.removeEventListener('pointerdown', pointerDown);
    container.removeEventListener('pointermove', pointerMove);
    container.removeEventListener('pointerup', pointerUp);
    container.removeEventListener('pointercancel', pointerCancel);
    desktop.removeEventListener('change', configure);
    container.removeEventListener('click', click);
    container.removeEventListener('dragstart', dragStart);
    container.removeEventListener('dragover', dragOver);
    container.removeEventListener('drop', drop);
    container.removeEventListener('dragend', dragEnd);
  };
}
