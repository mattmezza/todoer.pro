import { confirmAction } from './confirm.js';

export async function trashTask(api, task, element, signal) {
  const confirmed = await confirmAction(
    'Move this task and all its sub-tasks to trash? Unsaved drafts remain on this device.',
  );
  if (!confirmed || signal.aborted) return false;
  await api(`/tasks/${encodeURIComponent(task.id)}/trash`, 'POST', {
    revision: task.revision,
  });
  if (element.isConnected && !matchMedia('(prefers-reduced-motion: reduce)').matches) {
    const row = element.matches('.task, .task-page-child');
    await element
      .animate(
        [
          {
            opacity: 1,
            transform: 'translateX(0)',
            ...(row && { height: `${element.offsetHeight}px`, overflow: 'hidden' }),
          },
          {
            opacity: 0,
            transform: 'translateX(16px)',
            ...(row && {
              height: '0px',
              minHeight: '0px',
              paddingBlock: '0px',
              marginBlock: '0px',
              borderBlockWidth: '0px',
              overflow: 'hidden',
            }),
          },
        ],
        {
          id: 'task-trash',
          duration: 200,
          easing: 'cubic-bezier(.2,.7,.2,1)',
          fill: 'forwards',
        },
      )
      .finished.catch(() => {});
  }
  return !signal.aborted;
}
