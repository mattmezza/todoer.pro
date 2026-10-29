// Keep filters and the current page while checking for updated delivery state.
document
  .querySelector('[data-refresh-deliveries]')
  ?.addEventListener('click', () => location.reload());
