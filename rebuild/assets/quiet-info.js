// Routine draft reassurance stays available without interrupting the task view.
const messages = new Map();
let widget;
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
  if (!widget || !messages.size) return;
  const host = [...document.querySelectorAll('dialog[open]')].at(-1) || document.body;
  if (widget.parentElement !== host) host.append(widget);
}
function render() {
  if (!messages.size) {
    widget?.remove();
    return;
  }
  if (!widget) {
    widget = document.createElement('details');
    widget.className = 'quiet-info';
    widget.innerHTML =
      '<summary aria-label="Information about local drafts" title="Information"><svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M12 11v6m0-10v1"/></svg></summary><div class="quiet-info-content"><strong>Information</strong><ul></ul></div>';
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
  const list = widget.querySelector('ul');
  list.replaceChildren(
    ...[...messages.values()].map((text) =>
      Object.assign(document.createElement('li'), { textContent: text }),
    ),
  );
  place();
}
