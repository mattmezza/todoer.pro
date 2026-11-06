import { refreshPage } from './page-navigation.js';
import { createScope } from './page-lifecycle.js';

export function workspaceCreateDialog() {
  let dialog;
  return {
    init() {
      dialog = this.$el;
    },
    openFromTrigger(event) {
      if (!event.target.closest('[data-workspace-create-open]')) return;
      event.preventDefault();
      dialog.showModal();
      dialog.querySelector('[name=title]').focus();
    },
    close() {
      dialog.close();
    },
  };
}

export function workspaceActionDialog() {
  const scope = createScope();
  let dialog;
  let workspace;
  return {
    action: '',
    confirmName: false,
    confirmation: '',
    busy: false,
    error: '',
    title: '',
    init() {
      dialog = this.$el;
    },
    destroy() {
      scope.dispose();
    },
    get heading() {
      return this.action === 'delete'
        ? 'Delete archived workspace?'
        : this.action === 'archive'
          ? 'Archive workspace?'
          : 'Restore workspace?';
    },
    get message() {
      if (this.confirmName) return `Type “${this.title}” to confirm.`;
      return this.action === 'delete'
        ? 'This removes the workspace from use and revokes access to its lists. Data and files are retained in trash. Continue to confirm its name.'
        : 'Tasks and history are preserved.';
    },
    get acceptLabel() {
      return this.action === 'delete'
        ? this.confirmName
          ? 'Delete workspace'
          : 'Continue'
        : this.action === 'archive'
          ? 'Archive'
          : 'Restore';
    },
    openFromTrigger(event) {
      const button = event.target.closest('[data-workspace-action]');
      if (!button || this.busy) return;
      const row = button.closest('[data-workspace]');
      workspace = { id: row.dataset.workspace, revision: Number(row.dataset.revision) };
      this.title = row.dataset.title;
      this.action = button.dataset.workspaceAction;
      this.confirmName = false;
      this.confirmation = '';
      this.error = '';
      dialog.showModal();
    },
    close() {
      dialog.close();
    },
    async save() {
      if (this.busy) return;
      if (this.action === 'delete' && !this.confirmName) {
        this.confirmName = true;
        this.$nextTick(() => this.$refs.confirmation.focus());
        return;
      }
      this.busy = true;
      this.error = '';
      try {
        const response = await scope.request(
          `/api/v1/workspaces/${workspace.id}/lifecycle`,
          {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json',
              'X-CSRF-Token': document.querySelector('meta[name="csrf-token"]').content,
            },
            body: JSON.stringify({
              revision: workspace.revision,
              action: this.action,
              confirmation: this.action === 'delete' ? this.confirmation : null,
            }),
          },
        );
        if (!response.ok)
          throw Error(
            (await response.json()).error?.message || 'Could not update workspace',
          );
        if (!scope.signal.aborted) refreshPage();
      } catch (error) {
        if (!scope.signal.aborted) this.error = error.message;
      } finally {
        this.busy = false;
      }
    },
  };
}
