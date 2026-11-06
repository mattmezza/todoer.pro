import { request } from './request.js';
import { toast } from './toast.js';

const navigation = document.querySelector('#navigation');
const container = document.querySelector('#task-list');
if (container && navigation) {
  const collapsed = new Set(JSON.parse(navigation.dataset.collapsedSections || '[]'));
  const initialized = new WeakSet();
  let writes = Promise.resolve();
  function mount() {
    for (const details of container.querySelectorAll('.section-collapse')) {
      if (initialized.has(details)) continue;
      initialized.add(details);
      const section = details.closest('[data-section-id]');
      const id = section.dataset.sectionId;
      details.open = !collapsed.has(id);
      details
        .querySelector('summary')
        .setAttribute('aria-label', `Toggle section ${section.dataset.sectionTitle}`);
      // Toggle events also fire after programmatic updates; compare with saved state.
      details.addEventListener('toggle', () => {
        const next = !details.open;
        if (collapsed.has(id) === next) return;
        if (next) collapsed.add(id);
        else collapsed.delete(id);
        writes = writes
          .catch(() => {})
          .then(async () => {
            try {
              const response = await request(
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
              toast(`${error.message} Your choice applies on this page only.`);
            }
          });
      });
    }
  }
  mount();
  new MutationObserver(mount).observe(container, { childList: true, subtree: true });
}
