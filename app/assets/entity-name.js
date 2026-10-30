// Match the server's slugify rules without changing the user's input or caret.
export function referenceSlug(value) {
  const slug = value
    .toLowerCase()
    .replace(/[^\p{Alphabetic}\p{Number}]+/gu, '-')
    .replace(/^-|-$/g, '');
  return [...slug].slice(0, 64).join('').replace(/-+$/, '') || 'untitled';
}
const selector = 'input[data-reference-prefix], textarea[data-reference-prefix]';
function preview(field) {
  if (!field.matches(selector)) return;
  let output = document.getElementById(field.dataset.referencePreviewId);
  if (!output) {
    output = document.createElement('span');
    output.id = `reference-preview-${crypto.randomUUID()}`;
    output.className = 'entity-name-reference';
    output.setAttribute('aria-live', 'polite');
    output.title =
      'The saved reference may include a suffix if this name is already in use.';
    field.dataset.referencePreviewId = output.id;
    field.setAttribute(
      'aria-describedby',
      [field.getAttribute('aria-describedby'), output.id].filter(Boolean).join(' '),
    );
  }
  const actions = field
    .closest('[data-list-edit], [data-space-edit-form]')
    ?.querySelector('.inline-actions');
  if (actions) {
    if (output.parentElement !== actions) actions.prepend(output);
  } else {
    const anchor = field.closest('label') || field;
    if (anchor.nextElementSibling !== output) anchor.after(output);
  }
  output.textContent = `Reference preview: ${field.dataset.referencePrefix}${referenceSlug(field.value)}`;
  output.hidden = !field.value.trim();
}
for (const field of document.querySelectorAll(selector)) preview(field);
for (const event of ['input', 'focusin', 'compositionend'])
  document.addEventListener(event, ({ target }) => {
    if (target instanceof Element) preview(target);
  });
document.addEventListener('reset', (event) => {
  queueMicrotask(() => {
    for (const field of event.target.querySelectorAll(selector)) preview(field);
  });
});
