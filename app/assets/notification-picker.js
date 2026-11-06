export function mutedLists() {
  let root;
  return {
    choices: [],
    query: '',
    resultsOpen: false,
    announcement: '',
    init() {
      root = this.$el;
      this.choices = [...root.querySelectorAll('[data-mute-label]')].map((label) => {
        const input = label.querySelector('input');
        const title = label.querySelector('[data-mute-title]').textContent;
        const space = label.dataset.space;
        return {
          input,
          name: input.name,
          title,
          space,
          muted: input.checked,
          muteLabel: `Mute ${title} in ${space}`,
          unmuteLabel: `Unmute ${title} in ${space}`,
        };
      });
    },
    get selected() {
      return this.choices.filter((choice) => choice.muted);
    },
    get matching() {
      const query = this.query.trim().toLocaleLowerCase();
      return this.choices.filter(
        (choice) =>
          !choice.muted &&
          `${choice.title} ${choice.space}`.toLocaleLowerCase().includes(query),
      );
    },
    get matches() {
      return this.matching.slice(0, 8);
    },
    get hint() {
      if (this.matching.length > 8) return 'Type to narrow down the results.';
      if (this.matching.length) return '';
      return this.query.trim() ? 'No matching lists.' : 'All available lists are muted.';
    },
    get countLabel() {
      return `${this.selected.length} muted`;
    },
    get emptyLabel() {
      return this.choices.length
        ? 'All your lists can notify you.'
        : 'No lists to mute yet.';
    },
    showResults() {
      this.resultsOpen = true;
    },
    mute(event) {
      this.change(event.currentTarget.dataset.name, true);
    },
    unmute(event) {
      this.change(event.currentTarget.dataset.name, false);
    },
    change(name, muted) {
      const choice = this.choices.find((choice) => choice.name === name);
      choice.muted = muted;
      choice.input.checked = muted;
      choice.input.dispatchEvent(new Event('change', { bubbles: true }));
      this.query = '';
      this.$refs.search.focus();
      this.resultsOpen = false;
      this.announcement = `${choice.title} ${muted ? 'muted' : 'unmuted'}. Save preferences to apply.`;
    },
    focusOut(event) {
      if (!root.contains(event.relatedTarget)) this.resultsOpen = false;
    },
    searchKey(event) {
      if (event.key === 'Enter') {
        event.preventDefault();
        if (this.resultsOpen) this.$refs.results.querySelector('button')?.click();
      } else if (event.key === 'ArrowDown') {
        event.preventDefault();
        this.resultsOpen = true;
        this.$nextTick(() => this.$refs.results.querySelector('button')?.focus());
      } else if (event.key === 'Escape') {
        event.preventDefault();
        this.query = '';
        this.resultsOpen = false;
      }
    },
  };
}
