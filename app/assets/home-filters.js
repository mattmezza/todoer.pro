export function priorityFilters() {
  let root;
  return {
    selected: [],
    priorities: [
      { value: 'all', label: 'All priorities' },
      ...[0, 1, 2, 3].map((value) => ({ value: String(value), label: `P${value}` })),
    ],
    rowPriorities: [],
    init() {
      root = this.$el;
      this.readSelection();
      this.rowsChanged();
    },
    get empty() {
      return (
        this.rowPriorities.length > 0 &&
        !this.rowPriorities.some((priority) => this.matches(priority))
      );
    },
    matches(priority) {
      return !this.selected.length || this.selected.includes(priority);
    },
    matchesRow(row) {
      return this.matches(row.querySelector('[data-priority]')?.dataset.priority || '3');
    },
    pressed(value) {
      return value === 'all' ? !this.selected.length : this.selected.includes(value);
    },
    readSelection() {
      this.selected = (new URLSearchParams(location.search).get('prio') || '')
        .split(',')
        .filter((value) => /^[0-3]$/.test(value));
      this.syncLinks();
    },
    togglePriority(event) {
      const value = event.currentTarget.dataset.priority;
      this.selected =
        value === 'all'
          ? []
          : this.selected.includes(value)
            ? this.selected.filter((priority) => priority !== value)
            : [...this.selected, value];
      if (this.selected.length === 4) this.selected = [];
      const url = new URL(location.href);
      this.updateURL(url);
      history.pushState({ htmx: true }, '', url);
      this.syncLinks();
    },
    updateURL(url) {
      if (this.selected.length)
        url.searchParams.set('prio', [...this.selected].sort().join(','));
      else url.searchParams.delete('prio');
    },
    syncLinks() {
      for (const link of root.querySelectorAll(
        '.planning-tabs a,.planning-space-filters a,.planning-zone a',
      )) {
        const url = new URL(link.href, location.href);
        this.updateURL(url);
        link.href = url;
      }
    },
    rowsChanged() {
      this.rowPriorities = [...root.querySelectorAll('[data-planning-task]')].map(
        (row) => row.querySelector('[data-priority]')?.dataset.priority || '3',
      );
      this.readSelection();
    },
  };
}
