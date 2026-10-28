(() => {
  const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
  if (!reduced && 'IntersectionObserver' in window) {
    const observer = new IntersectionObserver(entries => {
      entries.forEach(entry => {
        if (entry.isIntersecting) {
          entry.target.classList.add('revealed');
          observer.unobserve(entry.target);
        }
      });
    }, { threshold: 0.08 });
    document.querySelectorAll('.block, .preview, .plan, .founder-card, .contact-grid').forEach(el => {
      if (el.getBoundingClientRect().top > innerHeight) {
        el.classList.add('will-reveal'); observer.observe(el);
        setTimeout(() => el.classList.add('revealed'), 4500);
      }
    });
  }
  const buttons = [...document.querySelectorAll('[data-choose-currency]')];
  const panels = [...document.querySelectorAll('[data-currency]')];
  if (buttons.length) {
    function choose(currency) {
      panels.forEach(el => { el.hidden = el.dataset.currency !== currency; });
      buttons.forEach(el => el.setAttribute('aria-pressed', String(el.dataset.chooseCurrency === currency)));
    }
    buttons.forEach(el => el.addEventListener('click', () => choose(el.dataset.chooseCurrency)));
    choose('EUR');
  }
})();
