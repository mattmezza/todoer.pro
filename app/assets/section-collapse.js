import { toast } from './toast.js';
import { pageScope } from './page-lifecycle.js';

// Persist user choices in order, including when live updates replace a heading.
// Requests belong to the page so replacing a disclosure cannot discard its save.
let writes = Promise.resolve();
export function sectionCollapse() {
  let disclosure;
  let collapsed;
  let scope;
  let id;
  return {
    init() {
      disclosure = this.$el;
      collapsed = !disclosure.open;
      scope = pageScope();
      id = disclosure.closest('[data-section-id]').dataset.sectionId;
    },
    changed() {
      const next = !disclosure.open;
      // Native toggle events also fire after the server's initial open attribute.
      if (next === collapsed) return;
      collapsed = next;
      writes = writes
        .catch(() => {})
        .then(async () => {
          try {
            const response = await scope.request(
              `/api/v1/section-preferences/${encodeURIComponent(id)}`,
              {
                method: 'PUT',
                headers: {
                  'Content-Type': 'application/json',
                  'X-CSRF-Token': document.querySelector('meta[name="csrf-token"]')
                    .content,
                },
                body: JSON.stringify({ collapsed: next }),
              },
            );
            if (!response.ok) throw Error('Could not save section visibility.');
          } catch (error) {
            if (!scope.signal.aborted)
              toast(`${error.message} Your choice applies on this page only.`);
          }
        });
    },
  };
}
