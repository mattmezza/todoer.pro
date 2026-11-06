import { beginLoading } from './loading.js';
import { toast } from './toast.js';
import { createScope } from './page-lifecycle.js';

export function capturePreferences() {
  const scope = createScope();
  let form;
  return {
    loaded: false,
    busy: true,
    smartEnabled: false,
    dateEnabled: false,
    repeatCapture: false,
    instructions: '',
    instructionsOpen: false,
    status: '',
    init() {
      form = this.$el;
      return this.load();
    },
    destroy() {
      scope.dispose();
    },
    async load() {
      const skeleton = beginLoading(form, { variant: 'form' });
      scope.cleanup(() => skeleton.finish());
      this.busy = true;
      this.status = '';
      try {
        const response = await scope.request('/api/v1/capture/preferences', {
          cache: 'no-store',
        });
        if (!response.ok) throw Error('Could not load preferences. Please try again.');
        const value = await response.json();
        if (scope.signal.aborted) return;
        this.smartEnabled = value.smart_enabled;
        this.dateEnabled = value.date_enabled;
        this.repeatCapture = value.repeat_capture;
        this.instructions = value.ai_instructions;
        this.instructionsOpen = Boolean(value.ai_instructions.trim());
        this.loaded = true;
      } catch (error) {
        if (scope.signal.aborted) return;
        this.status = error.message;
        skeleton.fail(error.message, () => this.load());
      } finally {
        skeleton.finish();
        this.busy = false;
      }
    },
    async save() {
      if (!this.loaded || this.busy) return;
      this.busy = true;
      this.status = 'Saving…';
      try {
        const response = await scope.request('/api/v1/capture/preferences', {
          method: 'PUT',
          headers: {
            'Content-Type': 'application/json',
            'X-CSRF-Token': document.querySelector('meta[name=csrf-token]').content,
          },
          body: JSON.stringify({
            smart_enabled: this.smartEnabled,
            date_enabled: this.dateEnabled,
            repeat_capture: this.repeatCapture,
            ai_instructions: this.instructions,
          }),
        });
        if (!response.ok)
          throw Error(
            (await response.json()).error?.message ||
              'Could not save preferences. Your changes are still here.',
          );
        if (scope.signal.aborted) return;
        this.status = '';
        toast('Preferences saved.');
      } catch (error) {
        if (!scope.signal.aborted) this.status = error.message;
      } finally {
        this.busy = false;
      }
    },
  };
}
