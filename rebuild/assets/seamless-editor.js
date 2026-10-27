// Capture before hiding labels, then preserve their typography and text origins.
export function editorAnchor(node) {
  const box = node.getBoundingClientRect();
  const style = getComputedStyle(node);
  return { box, font: style.font, letterSpacing: style.letterSpacing, lineHeight: style.lineHeight, color: style.color };
}
export function alignEditorField(field, anchor) {
  Object.assign(field.style, {
    font: anchor.font, letterSpacing: anchor.letterSpacing, lineHeight: anchor.lineHeight,
    boxSizing: 'border-box', padding: '0', border: '0', borderRadius: '0',
    background: 'transparent', margin: '0', minHeight: '0', outline: 'none',
    display: 'block', width: `${anchor.box.width}px`, height: `${anchor.box.height}px`,
    color: 'var(--text)',
  });
  const current = field.getBoundingClientRect();
  field.style.marginLeft = `${anchor.box.left - current.left}px`;
  field.style.marginTop = `${anchor.box.top - current.top}px`;
  if (field.tagName === 'TEXTAREA') {
    const resize = () => { field.style.height = '0px'; field.style.height = `${Math.max(anchor.box.height, field.scrollHeight)}px`; };
    resize();
    if (!field.dataset.seamlessResize) {
      field.addEventListener('input', resize);
      field.dataset.seamlessResize = 'true';
    }
  }
}
