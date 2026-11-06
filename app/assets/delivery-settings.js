import { navigate, refreshPage } from './page-navigation.js';
import { onPageLoad } from './page-lifecycle.js';

onPageLoad((scope) => {
  // Keep filters and the current page while checking for updated delivery state.
  scope.listen(document.querySelector('[data-refresh-deliveries]'), 'click', () =>
    refreshPage(),
  );
});
