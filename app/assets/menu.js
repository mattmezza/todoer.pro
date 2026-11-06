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
      event.preventDefault();
      event.stopPropagation();
      this.close();
      menu.querySelector('summary')?.focus();
    },
  };
}
