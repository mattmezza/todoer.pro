import { toast } from './toast.js';
import { setSaveState } from './quiet-info.js';
import { createScope } from './page-lifecycle.js';

export function notificationPreferences() {
  const scope = createScope();
  let form;
  return {
    busy: false,
    init() {
      form = this.$el;
    },
    destroy() {
      scope.dispose();
    },
    async save() {
      if (this.busy) return;
      const payload = new URLSearchParams(new FormData(form));
      this.busy = true;
      setSaveState('notification-preferences', 'saving');
      try {
        const response = await scope.request(form.action, {
          method: 'POST',
          body: payload,
        });
        if (!response.ok) {
          const data = await response.json().catch(() => null);
          throw Error(data?.error?.message || 'Could not save notification preferences.');
        }
        if (!scope.signal.aborted) {
          setSaveState('notification-preferences', 'saved');
          toast('Notification preferences saved.');
        }
      } catch (error) {
        if (!scope.signal.aborted)
          setSaveState('notification-preferences', 'error', error.message);
      } finally {
        this.busy = false;
      }
    },
  };
}
