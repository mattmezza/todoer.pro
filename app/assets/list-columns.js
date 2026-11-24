import { replacePageURL } from './page-navigation.js';

export function mountListColumns({
  scope,
  container,
  listId,
  csrf,
  canWrite,
  api,
  notice,
  blocked,
  refresh,
  resumeRefresh,
}) {
  const toggle = document.querySelector('[data-switch-list-view]');
  const save = document.querySelector('[data-save-list-view]');
  const menu = toggle.closest('details');
  const isColumns = () => container.dataset.listView === 'columns';
  let columnsDefault = container.dataset.columnsDefault === 'true';
  let saving = false;
  function syncViewURL() {
    const url = new URL(location.href);
    if (isColumns() === columnsDefault) url.searchParams.delete('view');
    else url.searchParams.set('view', container.dataset.listView);
    if (url.href !== location.href) replacePageURL(url);
  }
  function updateMenu() {
    const columns = isColumns();
    toggle.textContent = columns ? 'Show list' : 'Show columns';
    save.textContent = columns ? 'Use columns as default' : 'Use list as default';
    save.hidden = columns === columnsDefault;
    save.disabled = saving;
    container.tabIndex = columns ? 0 : -1;
  }
  function closeMenu() {
    menu.open = false;
    menu.querySelector('summary').focus();
  }
  updateMenu();
  scope.listen(toggle, 'click', () => {
    container.dataset.listView = isColumns() ? 'list' : 'columns';
    syncViewURL();
    updateMenu();
    closeMenu();
  });
  scope.listen(save, 'click', async () => {
    if (saving) return;
    const columns = isColumns();
    saving = true;
    updateMenu();
    closeMenu();
    try {
      const response = await scope.request(
        `/api/v1/lists/${encodeURIComponent(listId)}/view-preference`,
        {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': csrf },
          body: JSON.stringify({ columns }),
        },
      );
      if (!response.ok) throw Error('Could not save the default view.');
      columnsDefault = columns;
      syncViewURL();
      notice(columns ? 'Columns saved as the default.' : 'List saved as the default.');
    } catch (error) {
      notice(error.message, true);
    } finally {
      saving = false;
      updateMenu();
    }
  });
  if (!canWrite) return;

  let drag, moving, suppressClick;
  function targetAt(x, y) {
    const column = document.elementFromPoint(x, y)?.closest('.task-section');
    return column?.parentElement === container && column !== drag.source ? column : null;
  }
  function markTarget(target) {
    if (drag.target === target) return;
    drag.target?.removeAttribute('data-column-drop');
    drag.target = target;
    target?.setAttribute('data-column-drop', '');
  }
  function scrollWhileDragging(current) {
    if (drag !== current) return;
    const { left, right } = container.getBoundingClientRect();
    const { x, y } = current;
    if (x >= left - 40 && x <= right + 40) {
      if (x < left + 34) container.scrollLeft -= 12;
      else if (x > right - 34) container.scrollLeft += 12;
    }
    markTarget(targetAt(x, y));
    scope.frame(() => scrollWhileDragging(current));
  }
  function stopDrag() {
    if (!drag) return;
    markTarget(null);
    drag.row.removeAttribute('data-column-dragging');
    drag = null;
    resumeRefresh();
  }
  scope.listen(
    document,
    'pointerdown',
    () => {
      suppressClick = false;
      stopDrag();
    },
    true,
  );
  scope.listen(container, 'pointerdown', (event) => {
    if (!isColumns() || moving || !event.isPrimary || event.button !== 0) return;
    const row = event.target.closest('.task[data-id]');
    if (!row) return;
    const grip = event.target.closest('[data-touch-order]');
    if (
      !grip &&
      (event.pointerType !== 'mouse' ||
        event.target.closest(
          'button, input, textarea, select, details, form, .task-description, a:not(.task-title)',
        ) ||
        event.altKey ||
        event.ctrlKey ||
        event.metaKey ||
        event.shiftKey)
    )
      return;
    drag = {
      pointerId: event.pointerId,
      row,
      source: row.closest('.task-section'),
      startX: event.clientX,
      startY: event.clientY,
      revision: Number(row.dataset.revision),
    };
  });
  scope.listen(container, 'dragstart', (event) => {
    if (drag) event.preventDefault();
  });
  scope.listen(document, 'pointermove', (event) => {
    if (event.pointerId !== drag?.pointerId) return;
    if (event.pointerType === 'mouse' && !event.buttons) return stopDrag();
    drag.x = event.clientX;
    drag.y = event.clientY;
    if (!drag.started) {
      if (Math.hypot(drag.x - drag.startX, drag.y - drag.startY) < 8) return;
      suppressClick = true;
      if (blocked()) {
        notice(
          'Finish the active edit or close the task details before moving a card.',
          true,
        );
        stopDrag();
        return;
      }
      drag.started = true;
      drag.row.setAttribute('data-column-dragging', '');
      scrollWhileDragging(drag);
    }
    markTarget(targetAt(drag.x, drag.y));
  });
  scope.listen(document, 'pointerup', async (event) => {
    if (event.pointerId !== drag?.pointerId) return;
    const { row, revision, started } = drag;
    const target = started && targetAt(event.clientX, event.clientY);
    stopDrag();
    if (!target) return;
    moving = true;
    try {
      await api(`/tasks/${encodeURIComponent(row.dataset.id)}/section`, 'PUT', {
        revision,
        section_id: target.dataset.sectionId || null,
      });
      notice('');
      refresh();
    } catch (error) {
      notice(error.message || 'Could not move the card.', true);
      refresh();
    } finally {
      moving = false;
    }
  });
  scope.listen(document, 'pointercancel', (event) => {
    if (event.pointerId === drag?.pointerId) stopDrag();
  });
  scope.listen(window, 'blur', stopDrag);
  scope.listen(
    container,
    'click',
    (event) => {
      if (!isColumns()) return;
      if (suppressClick && event.detail !== 0) {
        event.preventDefault();
        event.stopImmediatePropagation();
        return;
      }
      event.target
        .closest('[data-touch-order]')
        ?.closest('.task')
        ?.querySelector('[data-task-move-open]')
        ?.click();
    },
    true,
  );
}
