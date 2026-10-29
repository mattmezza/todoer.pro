const messages = new Map(),
  saves = new Map(),
  timers = new Map();
let widget;
const weights = { info: 0, saved: 1, unsaved: 2, saving: 3, error: 4 };
export function setSaveState(source, state, message = '') {
  clearTimeout(timers.get(source));
  saves.set(source, { state, message });
  if (saves.size > 12) saves.delete(saves.keys().next().value);
  if (state === 'saved')
    timers.set(
      source,
      setTimeout(() => {
        const value = saves.get(source);
        if (value?.state === 'saved') {
          value.state = 'info';
          render();
        }
      }, 1800),
    );
  render();
}
export async function trackSave(source, action) {
  setSaveState(source, 'saving', 'Saving your changes…');
  try {
    const result = await action();
    setSaveState(source, 'saved', 'Your changes were saved.');
    return result;
  } catch (error) {
    setSaveState(
      source,
      'error',
      'Changes were not confirmed: ' +
        error.message +
        ' Your local draft is kept. Review saved data before retrying.',
    );
    throw error;
  }
}
export function stowInfo(message, source) {
  if (!message) {
    messages.delete(source);
    render();
    return false;
  }
  const routine =
    /draft.*(?:kept|preserved|restored|original revision)|(?:planning|metadata) draft keeps|local draft|draft.*this device|save automatically/i.test(
      message,
    );
  const actionable =
    /could not|not confirmed|unavailable|failed|conflict|review|retry|copy .*before|not saved/i.test(
      message,
    );
  if (!routine || actionable) return false;
  messages.set(source, message);
  if (messages.size > 8) messages.delete(messages.keys().next().value);
  render();
  return true;
}
function place() {
  if (!widget || (!messages.size && !saves.size)) return;
  const host = [...document.querySelectorAll('dialog[open]')].at(-1) || document.body;
  if (widget.parentElement !== host) host.append(widget);
}
function render() {
  if (!messages.size && !saves.size) {
    widget?.remove();
    return;
  }
  if (!widget) {
    widget = document.createElement('details');
    widget.className = 'quiet-info';
    widget.innerHTML =
      '<summary title="Information"><svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><circle class="quiet-ring" cx="12" cy="12" r="9"/><g class="quiet-symbol quiet-symbol-info"><path d="M12 11v6m0-10v1"/></g><g class="quiet-symbol quiet-symbol-unsaved"><circle cx="12" cy="12" r="3" fill="currentColor" stroke="none"/></g><g class="quiet-symbol quiet-symbol-saving"><path d="M12 3a9 9 0 0 1 9 9" stroke-linecap="round"/></g><g class="quiet-symbol quiet-symbol-saved"><path d="m7 12 3 3 7-7" stroke-linecap="round" stroke-linejoin="round"/></g><g class="quiet-symbol quiet-symbol-error"><path d="M12 7v6m0 3v1" stroke-linecap="round"/></g></svg></summary><div class="quiet-info-content"><strong>Information</strong><ul></ul></div><span class="sr-only" role="status" aria-live="polite"></span>';
    document.addEventListener('click', (event) => {
      if (!widget.contains(event.target)) widget.open = false;
    });
    widget.addEventListener('keydown', (event) => {
      if (event.key === 'Escape' && widget.open) {
        event.preventDefault();
        event.stopPropagation();
        widget.open = false;
        widget.querySelector('summary').focus();
      }
    });
    new MutationObserver(place).observe(document.body, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ['open'],
    });
  }
  const current = [...saves.values()].sort(
    (a, b) => weights[b.state] - weights[a.state],
  )[0] || { state: 'info' };
  const labels = {
    info: 'Information',
    unsaved: 'Unsaved changes',
    saving: 'Saving changes',
    saved: 'Changes saved',
    error: 'Changes need attention',
  };
  widget.dataset.saveState = current.state;
  const summary = widget.querySelector('summary');
  summary.setAttribute('aria-label', labels[current.state]);
  summary.title = labels[current.state] + ' · Click for details';
  widget.querySelector('strong').textContent = labels[current.state];
  widget.querySelector('[role=status]').textContent = labels[current.state];
  const details = [
    ...new Set(
      [...saves.values()]
        .map((value) => value.message)
        .filter(Boolean)
        .concat([...messages.values()]),
    ),
  ];
  widget
    .querySelector('ul')
    .replaceChildren(
      ...details.map((text) =>
        Object.assign(document.createElement('li'), { textContent: text }),
      ),
    );
  place();
}
