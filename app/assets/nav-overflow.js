// Keep the accessible label unchanged; animate only its visual contents.
const sidebar = document.querySelector('#navigation');
if (sidebar) {
  const labels = new WeakSet();
  const observer = new ResizeObserver((entries) => {
    for (const { target } of entries) update(target);
  });
  function update(label) {
    const text = label.firstElementChild;
    if (!text || !label.clientWidth) return;
    const distance = Math.max(0, text.scrollWidth - label.clientWidth);
    label.toggleAttribute('data-overflowing', distance > 2);
    label.style.setProperty('--nav-scroll-distance', `${distance}px`);
    label.style.setProperty(
      '--nav-scroll-duration',
      `${Math.max(12, Math.min(28, 9 + distance / 13))}s`,
    );
  }
  function scan() {
    for (const link of sidebar.querySelectorAll('nav .nav-link,.space-switch-item')) {
      let label = [...link.children]
        .reverse()
        .find((child) =>
          child.matches('span:not(.list-dot):not(.favorite-space-avatar)'),
        );
      if (!label && link.classList.contains('space-switch-item')) {
        const text = [...link.childNodes].find(
          (node) => node.nodeType === Node.TEXT_NODE && node.textContent.trim(),
        );
        if (!text) continue;
        label = document.createElement('span');
        text.replaceWith(label);
        label.append(text);
      }
      if (!label || labels.has(label)) continue;
      labels.add(label);
      const inner = document.createElement('span');
      inner.className = 'nav-overflow-inner';
      inner.append(...label.childNodes);
      label.append(inner);
      label.classList.add('nav-overflow-label');
      const text = inner.textContent || '';
      let hash = 0;
      for (const character of text) hash = (hash * 31 + character.codePointAt(0)) >>> 0;
      label.style.setProperty('--nav-scroll-delay', `${2 + (hash % 90) / 10}s`);
      observer.observe(label);
      update(label);
    }
  }
  new MutationObserver(scan).observe(sidebar, { childList: true, subtree: true });
  document.fonts?.ready.then(scan);
  scan();
}
