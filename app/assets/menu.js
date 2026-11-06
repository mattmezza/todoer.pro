export function dismissibleMenu() {
  let menu;
  return {
    init() {
      menu = this.$el;
    },
    close() {
      menu.open = false;
    },
    escape(event) {
      if (!menu.open || event.defaultPrevented) return;
      // A modal owns Escape while it is open. Otherwise dismiss the menu before
      // document-level panel and mobile-drawer handlers see the same key.
      if (document.querySelector('dialog[open]') && !menu.closest('dialog[open]')) return;
      event.preventDefault();
      event.stopPropagation();
      this.close();
      menu.querySelector('summary')?.focus();
    },
  };
}
