// The service queues callers; this component owns the native dialog and its UI.
export function confirmationDialog() {
  let dialog;
  let resolve;
  return {
    title: '',
    message: '',
    acceptLabel: '',
    destructive: false,
    init() {
      dialog = this.$el;
    },
    present(message, options) {
      this.title = options.title || 'Confirm change';
      this.message = message;
      this.destructive = /delete|discard|archive|trash|sign out/i.test(message);
      this.acceptLabel =
        options.acceptLabel ||
        (/delete/i.test(message)
          ? 'Delete'
          : /archive/i.test(message)
            ? 'Archive'
            : /trash/i.test(message)
              ? 'Move to trash'
              : /discard/i.test(message)
                ? 'Discard'
                : 'Confirm');
      dialog.returnValue = '';
      const result = new Promise((done) => {
        resolve = done;
      });
      this.$nextTick(() => {
        if (dialog.isConnected && resolve) dialog.showModal();
      });
      return result;
    },
    closed() {
      resolve?.(dialog.returnValue === 'confirm');
      resolve = null;
    },
    destroy() {
      resolve?.(false);
      resolve = null;
    },
  };
}
