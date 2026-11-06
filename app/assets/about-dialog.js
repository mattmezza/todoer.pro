import { createScope } from './page-lifecycle.js';
import { beginLoading } from './loading.js';

export function aboutDialog() {
  const scope = createScope();
  let dialog;
  let generation = 0;
  let loading;
  return {
    release: '',
    init() {
      dialog = this.$el;
    },
    destroy() {
      generation++;
      loading?.finish();
      scope.dispose();
    },
    async openFromTrigger(event) {
      if (!event.target.closest('[data-about-open]')) return;
      event.preventDefault();
      document.querySelector('.account-menu')?.removeAttribute('open');
      dialog.showModal();
      const current = ++generation;
      this.release = '';
      loading?.finish();
      const skeleton = (loading = beginLoading(this.$refs.release, { variant: 'text' }));
      try {
        const response = await scope.request('/api/v1/capabilities', {
          cache: 'no-store',
        });
        if (!response.ok) throw Error();
        const info = await response.json();
        if (current === generation && !scope.signal.aborted)
          this.release = `Release v${info.version.replace(/^v/, '')}`;
      } catch {
        if (current === generation && !scope.signal.aborted)
          this.release = 'Release information unavailable';
      } finally {
        skeleton.finish();
      }
    },
    close() {
      dialog.close();
    },
    closed() {
      generation++;
      loading?.finish();
      document.querySelector('.account-menu > summary')?.focus();
    },
  };
}
