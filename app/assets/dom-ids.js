// A task page and its panel can contain the same forms. Keep labels, form
// ownership and ARIA references local when mounting another copy of the markup.
export function scopeElementIds(root) {
  const suffix = crypto.randomUUID();
  const ids = new Map();
  for (const element of root.querySelectorAll('[id]')) {
    const original = element.id;
    element.id = `${original}-${suffix}`;
    ids.set(original, element.id);
  }
  for (const element of root.querySelectorAll(
    '[for],[form],[aria-labelledby],[aria-describedby]',
  )) {
    for (const attribute of ['for', 'form', 'aria-labelledby', 'aria-describedby']) {
      if (!element.hasAttribute(attribute)) continue;
      element.setAttribute(
        attribute,
        element
          .getAttribute(attribute)
          .split(/\s+/)
          .map((id) => ids.get(id) || id)
          .join(' '),
      );
    }
  }
}
