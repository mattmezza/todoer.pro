import { test, expect } from '@playwright/test';

// Deterministic network stalls; worker behavior is tested separately against the
// actual handlers, so interception here must not be bypassed by a worker.
test.use({ serviceWorkers: 'block', viewport: { width: 390, height: 844 } });

async function login(page) {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
}
function gate() {
  let release;
  const promise = new Promise((resolve) => {
    release = resolve;
  });
  return { promise, release };
}

test('a mobile tap shows a spinner before any destination HTML arrives', async ({
  page,
}) => {
  await login(page);
  const delayed = gate();
  await page.route('**/today', async (route) => {
    await delayed.promise;
    await route.continue().catch(() => {});
  });
  await page.getByRole('button', { name: 'Toggle navigation' }).click();
  const feedback = await page.evaluate(() => {
    const started = performance.now();
    document.querySelector('#navigation a[href="/today"]').click();
    const main = document.querySelector('#main');
    const spinner = main.querySelector('.loading-spinner');
    return {
      milliseconds: performance.now() - started,
      pending: main.classList.contains('navigation-pending'),
      spinner: !!spinner && spinner.getBoundingClientRect().width > 0,
      oldContentHidden: [...main.children]
        .filter((el) => !el.classList.contains('loading-skeleton'))
        .every((el) => getComputedStyle(el).display === 'none'),
    };
  });
  expect(feedback.pending).toBe(true);
  expect(feedback.spinner).toBe(true);
  expect(feedback.oldContentHidden).toBe(true);
  console.log(
    `Mobile navigation feedback: ${feedback.milliseconds.toFixed(1)}ms, before response`,
  );
  await expect(page.locator('#main .loading-spinner')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Toggle navigation' })).toHaveAttribute(
    'aria-expanded',
    'false',
  );
  await page.screenshot({
    path: `.local/rebuild/snappy-navigation-${test.info().project.name}.png`,
  });
  delayed.release();
  await expect(page).toHaveURL(/\/today$/);
  await expect(page.locator('#main')).not.toHaveClass(/navigation-pending/);
  await expect(page.locator('h1')).toHaveText('Today');
});

test('new-release HTML triggers a full navigation before new controllers mount', async ({
  page,
}) => {
  await login(page);
  await page.evaluate(() => {
    window.previousDocument = document;
  });
  let fullNavigation = false;
  await page.route('**/today', async (route) => {
    if (route.request().isNavigationRequest()) {
      fullNavigation = true;
      await route.continue();
      return;
    }
    const response = await route.fetch();
    const html = (await response.text()).replace(
      /name="asset-base" content="[^"]+"/,
      'name="asset-base" content="/static/next-release/"',
    );
    await route.fulfill({ response, body: html });
  });
  await page.evaluate(() =>
    document.querySelector('#navigation a[href="/today"]').click(),
  );
  await expect(page).toHaveURL(/\/today$/);
  await expect(page.locator('h1')).toHaveText('Today');
  expect(fullNavigation).toBe(true);
  expect(await page.evaluate(() => !!window.previousDocument)).toBe(false);
});
