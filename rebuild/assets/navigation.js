const navigation = document.querySelector('#navigation');
if (navigation) {
  for (const link of navigation.querySelectorAll('.settings-navigation .nav-link')) {
    const label = link.textContent.trim();
    link.setAttribute('aria-label', label);
    link.title = label;
    for (const child of [...link.childNodes]) {
      if (child.nodeType === Node.TEXT_NODE && child.textContent.trim()) {
        const caption = document.createElement('span');
        caption.className = 'nav-caption';
        caption.textContent = child.textContent;
        child.replaceWith(caption);
      }
    }
  }
  const desktop = matchMedia('(min-width: 1000px)');
  function update() {
    const automatic = desktop.matches && (document.body.classList.contains('task-settings-docked-open') || document.body.classList.contains('planning-detail-open'));
    const collapsed = automatic;
    if (document.body.classList.contains('navigation-collapsed') !== collapsed)
      document.body.classList.toggle('navigation-collapsed', collapsed);
  }
  new MutationObserver(update).observe(document.body, { attributes: true, attributeFilter: ['class'] });
  desktop.addEventListener('change', update);
  update();
}
