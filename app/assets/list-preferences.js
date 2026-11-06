export function listPreferences() {
  let root;
  let key;
  let cookieAccount;
  return {
    showCompleted: true,
    init() {
      root = this.$el;
      const account = document.querySelector('meta[name="account-id"]').content;
      key = `todoer:${account}:hide-completed`;
      cookieAccount = [...new TextEncoder().encode(account)]
        .map((byte) => byte.toString(16).padStart(2, '0'))
        .join('');
      try {
        this.showCompleted = localStorage.getItem(key) !== 'true';
      } catch {
        // Filtering remains usable when browser storage is unavailable.
      }
      this.syncCookie();
      this.$nextTick(() => {
        if (
          root.isConnected &&
          this.showCompleted &&
          root.querySelector('#task-list').dataset.hideCompleted === 'true'
        )
          this.refresh();
      });
    },
    get completedLabel() {
      return this.showCompleted ? 'Hide completed' : 'Show completed';
    },
    syncCookie() {
      document.cookie = `todoer_completed=${cookieAccount}.${this.showCompleted ? '0' : '1'}; Path=/; Max-Age=31536000; SameSite=Lax${location.protocol === 'https:' ? '; Secure' : ''}`;
    },
    refresh() {
      root.dispatchEvent(new CustomEvent('todoer:completed-filter', { bubbles: true }));
    },
    save() {
      this.syncCookie();
      try {
        localStorage.setItem(key, String(!this.showCompleted));
      } catch {
        // Keep this page's selection without persistent storage.
      }
      this.refresh();
    },
    toggleCompleted(event) {
      this.showCompleted = !this.showCompleted;
      event.currentTarget.closest('details').open = false;
      this.save();
    },
    storageChanged(event) {
      if (event.key !== key && event.key !== null) return;
      this.showCompleted = event.newValue !== 'true';
      this.save();
    },
  };
}
