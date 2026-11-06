import { onPageLoad } from './page-lifecycle.js';

// Delay only pointer activation of writable task rows. Replaying the original
// click preserves each surface's navigation, editing and completion behavior.
onPageLoad((scope) => {
  const rows =
    '.task[data-id],.workbench-task[data-task-id],[data-planning-task],.task-page-child[data-child-id],.task-page-heading';
  const interval = 300;
  let pending,
    pointer,
    replaying = false;
  function cancel() {
    if (!pending) return;
    clearTimeout(pending.timer);
    delete pending.row.dataset.activationPending;
    pending = null;
    scope.timeout(
      () => document.dispatchEvent(new Event('todoer:task-interaction-end')),
      0,
    );
  }
  scope.cleanup(cancel);
  scope.listen(
    document,
    'pointerdown',
    (event) => {
      pointer = {
        x: event.clientX,
        y: event.clientY,
        time: event.timeStamp,
        moved: false,
      };
    },
    true,
  );
  scope.listen(
    document,
    'pointermove',
    (event) => {
      if (pointer && Math.hypot(event.clientX - pointer.x, event.clientY - pointer.y) > 8)
        pointer.moved = true;
    },
    true,
  );
  scope.listen(
    document,
    'pointercancel',
    () => {
      pointer = null;
      cancel();
    },
    true,
  );
  scope.listen(document, 'contextmenu', cancel, true);
  scope.listen(
    document,
    'click',
    (event) => {
      if (replaying) return;
      const target = event.target;
      const row = target.closest?.(rows);
      const control = row?.querySelector(
        '.completion,.workbench-check,.task-child-complete,[data-root-complete]',
      );
      const link = target.closest?.('a');
      const title = target.closest?.('[data-edit-title]');
      if (
        !row ||
        !control ||
        control.disabled ||
        event.detail === 0 ||
        event.button !== 0 ||
        event.metaKey ||
        event.ctrlKey ||
        event.altKey ||
        event.shiftKey ||
        (pointer && (pointer.moved || event.timeStamp - pointer.time > 450)) ||
        target.closest(
          'input,textarea,select,summary,details,form,[contenteditable=true]',
        ) ||
        (target.closest('button') && !title) ||
        (link &&
          !link.matches('.task-title,.workbench-task-copy > a,.task-page-child-line > a'))
      ) {
        cancel();
        return;
      }
      event.preventDefault();
      event.stopImmediatePropagation();
      if (pending?.row === row && event.timeStamp - pending.time <= interval) {
        cancel();
        window.getSelection()?.removeAllRanges();
        if (control.getAttribute('aria-pressed') !== 'true') control.click();
        return;
      }
      cancel();
      row.dataset.activationPending = '';
      const activation = link || title || target;
      pending = {
        row,
        time: event.timeStamp,
        timer: scope.timeout(() => {
          cancel();
          if (!activation.isConnected) return;
          replaying = true;
          try {
            activation.click();
          } finally {
            replaying = false;
          }
        }, interval),
      };
    },
    true,
  );
  scope.listen(document, 'dblclick', (event) => {
    if (
      event.target.closest?.(rows) &&
      !event.target.closest('input,textarea,select,form,[contenteditable=true]')
    )
      event.preventDefault();
  });
});
