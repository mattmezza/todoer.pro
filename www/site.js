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
})();
