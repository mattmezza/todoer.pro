import { navigate } from './page-navigation.js';
import { createScope } from './page-lifecycle.js';

export function listCreateDialog() {
  const scope = createScope();
  let suggestionSequence = 0;
  let dialog;
  return {
    busy: false,
    error: '',
    init() {
      dialog = this.$el;
    },
    destroy() {
      scope.dispose();
    },
    openFromTrigger(event) {
      if (!event.target.closest('[data-list-create-open]')) return;
      event.preventDefault();
      document.querySelector('.account-menu')?.removeAttribute('open');
      if (!this.$refs.form.elements.workspace_id.value) {
        navigate('/spaces');
        return;
      }
      this.error = '';
      dialog.showModal();
      this.suggestColor();
      this.$refs.form.elements.title.focus();
    },
    close() {
      dialog.close();
    },
    closed() {
      suggestionSequence++;
      dialog.querySelector('.list-color-picker').open = false;
    },
    colorChanged() {
      suggestionSequence++;
    },
    async suggestColor() {
      const sequence = ++suggestionSequence;
      const space = this.$refs.form.elements.workspace_id.value;
      if (!space) return;
      try {
        const response = await scope.request(
          `/api/v1/workspaces/${encodeURIComponent(space)}/list-color-suggestion`,
        );
        if (!response.ok) return;
        const result = await response.json();
        if (sequence !== suggestionSequence || !dialog.open || scope.signal.aborted)
          return;
        const input = this.$refs.form.elements.color;
        input.value = result.color;
        input.dispatchEvent(new Event('input'));
      } catch {
        // The selected color stays usable when a suggestion cannot be loaded.
      }
    },
    async save() {
      if (this.busy) return;
      this.busy = true;
      this.error = '';
      const data = new FormData(this.$refs.form);
      try {
        const response = await scope.request('/api/v1/lists', {
          method: 'POST',
          credentials: 'same-origin',
          headers: {
            'Content-Type': 'application/json',
            'X-CSRF-Token': document.querySelector('meta[name="csrf-token"]')?.content,
          },
          body: JSON.stringify({
            workspace_id: data.get('workspace_id'),
            title: data.get('title'),
            description: data.get('description'),
            color: data.get('color'),
            restricted: false,
          }),
        });
        const result = await response.json();
        if (!response.ok) throw Error(result.error?.message || 'Could not create list');
        if (!scope.signal.aborted) navigate(`/lists/${result.id}`);
      } catch (error) {
        if (!scope.signal.aborted) this.error = error.message;
      } finally {
        this.busy = false;
      }
    },
  };
}
