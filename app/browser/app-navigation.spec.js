import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

async function signin(page) {
  // Avoid request routing in Firefox while exercising service-worker redirects.
  if (page.context().browser().browserType().name() === 'chromium')
    await page.route('https://fonts.bunny.net/**', (route) => route.abort());
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
  await page.evaluate(() => {
    window.navigationDocument = document;
    window.navigationSidebar = document.querySelector('#navigation');
    window.navigationHeader = document.querySelector('.mobile-header');
  });
}
async function open(page, path) {
  await page.evaluate(async (path) => {
    const { navigate } = await import('/assets/page-navigation.js');
    await navigate(path);
  }, path);
  await expect(page.locator('#main')).not.toHaveAttribute('aria-busy', 'true');
}
async function persistent(page) {
  expect(
    await page.evaluate(
      () =>
        window.navigationDocument === document &&
        window.navigationSidebar === document.querySelector('#navigation') &&
        window.navigationHeader === document.querySelector('.mobile-header'),
    ),
  ).toBe(true);
}

async function workerReady(page) {
  await page.evaluate(async () => {
    await navigator.serviceWorker.ready;
    if (!navigator.serviceWorker.controller)
      await new Promise((resolve) =>
        navigator.serviceWorker.addEventListener('controllerchange', resolve, {
          once: true,
        }),
      );
  });
}

test('navigation keeps the shell, URLs, titles and Back/Forward working', async ({
  page,
}) => {
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.addInitScript(() => {
    window.styleViolations = [];
    document.addEventListener('securitypolicyviolation', (event) => {
      if (event.effectiveDirective.startsWith('style-src'))
        window.styleViolations.push(event.blockedURI);
    });
  });
  await signin(page);
  // Indicator visibility comes from our stylesheet, without HTMX injecting CSS.
  expect(
    await page.evaluate(() => {
      const indicator = document.createElement('span');
      indicator.className = 'htmx-indicator';
      document.body.append(indicator);
      const hidden = getComputedStyle(indicator).visibility;
      indicator.classList.add('htmx-request');
      const visible = getComputedStyle(indicator).visibility;
      indicator.remove();
      return { hidden, visible, injected: !!window.htmx.indicatorStyleSheet };
    }),
  ).toEqual({ hidden: 'hidden', visible: 'visible', injected: false });
  const listUrl = page.url();
  await page.locator('#navigation a[href="/today"]').click();
  await expect(page).toHaveURL(/\/today$/);
  await expect(page.locator('h1')).toHaveText('Today');
  await expect(page).toHaveTitle('Today · Todoer');
  await page.locator('#navigation .account-menu summary').click();
  await page.locator('#navigation .account-menu a[href="/settings"]').click();
  await expect(page.locator('h1')).toHaveText('General settings');
  await page.getByRole('link', { name: 'Appearance', exact: true }).click();
  await expect(page.locator('h1')).toHaveText('Appearance');
  await expect(page.getByRole('link', { name: 'Appearance', exact: true })).toHaveClass(
    /active/,
  );
  await page.goBack();
  await expect(page.locator('h1')).toHaveText('General settings');
  await page.goBack();
  await expect(page.locator('h1')).toHaveText('Today');
  await page.goBack();
  await expect(page).toHaveURL(listUrl);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
  await page.goForward();
  await expect(page.locator('h1')).toHaveText('Today');
  await page.locator('#navigation a[href="/"]').click();
  await expect(page).toHaveURL(/\/inbox$/);
  await expect(page.locator('h1')).toHaveText('Inbox');
  await persistent(page);
  expect(errors).toEqual([]);
  expect(await page.evaluate(() => window.styleViolations)).toEqual([]);
});

test('repeated swaps clean up live connections and restore list capture drafts', async ({
  page,
}) => {
  await page.addInitScript(() => {
    const Source = window.EventSource;
    window.liveSources = new Set();
    window.EventSource = class extends Source {
      constructor(...args) {
        super(...args);
        window.liveSources.add(this);
      }
      close() {
        window.liveSources.delete(this);
        super.close();
      }
    };
  });
  await signin(page);
  const listUrl = page.url();
  await page.locator('#capture-open').click();
  await page.locator('#capture-title').fill('Unfinished navigation draft');
  for (let i = 0; i < 3; i++) {
    await open(page, '/today');
    await open(page, '/inbox');
    await open(page, listUrl);
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
    await expect(page.locator('#capture-title')).toHaveValue(
      'Unfinished navigation draft',
    );
    await expect.poll(() => page.evaluate(() => window.liveSources.size)).toBe(1);
    await expect(page.locator('.navigation-backdrop')).toHaveCount(1);
    await expect(page.locator('[data-notification-bell]')).toHaveCount(1);
  }
  await page.locator('#capture-title').fill('');
  await persistent(page);
});

test('failed navigation leaves the current screen and draft usable; latest navigation wins', async ({
  page,
}) => {
  await signin(page);
  const listUrl = page.url();
  await page.locator('#capture-open').click();
  await page.locator('#capture-title').fill('Keep this draft');
  await page.route('**/today', (route) =>
    route.fulfill({ status: 503, body: 'Unavailable' }),
  );
  await open(page, '/today');
  await expect(page).toHaveURL(listUrl);
  await expect(page.locator('#capture-title')).toHaveValue('Keep this draft');
  await page.unroute('**/today');
  let release;
  const waiting = new Promise((resolve) => {
    release = resolve;
  });
  await page.route('**/today', async (route) => {
    await waiting;
    await route.continue().catch(() => {});
  });
  await page.evaluate(() => {
    import('/assets/page-navigation.js').then(({ navigate }) => navigate('/today'));
  });
  await expect(page.locator('#main')).toHaveAttribute('aria-busy', 'true');
  await expect(page.locator('#capture-title')).toBeHidden();
  await expect(page.locator('#main .loading-spinner')).toBeVisible();
  expect(await page.locator('#main').evaluate((main) => main.inert)).toBe(true);
  await page.evaluate(() => {
    import('/assets/page-navigation.js').then(({ navigate }) => navigate('/inbox'));
  });
  release();
  await expect(page).toHaveURL(/\/inbox$/);
  await expect(page.locator('h1')).toHaveText('Inbox');
  await persistent(page);
});

test('a remounted list sends capture to its own destination and its task permalink works', async ({
  page,
}) => {
  await signin(page);
  await workerReady(page);
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const headers = { 'x-csrf-token': csrf };
  const space = await (
    await page.request.post('/api/v1/workspaces', {
      headers,
      data: { title: `Navigation space ${randomUUID()}` },
    })
  ).json();
  const response = await page.request.post('/api/v1/lists', {
    headers,
    data: { title: `Navigation ${randomUUID()}`, workspace_id: space.id },
  });
  expect(response.ok()).toBeTruthy();
  const list = await response.json();
  await open(page, '/today');
  await open(page, `/lists/${list.id}`);
  const title = `Captured after navigation ${randomUUID()}`;
  await page.locator('#capture-open').click();
  await page.locator('#capture-title').fill(title);
  await page.locator('#capture-form button[type=submit]').click();
  const row = page.locator('.task').filter({ hasText: title });
  await expect(row).toHaveCount(1);
  const taskId = await row.getAttribute('data-id');
  const saved = await (await page.request.get(`/api/v1/tasks/${taskId}`)).json();
  expect(saved.list_id).toBe(list.id);
  await open(page, `/tasks/${taskId}`);
  await expect(page.locator('[data-edit-title]')).toHaveText(title);
  await expect(page).toHaveURL(/\/tasks\//);
  await persistent(page);
});

test('mobile navigation closes the drawer and opens full task pages without reloading', async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await signin(page);
  await page.getByRole('button', { name: 'Toggle navigation' }).click();
  await page.locator('#navigation a[href="/inbox"]').click();
  await expect(page.locator('h1')).toHaveText('Inbox');
  await expect(page.getByRole('button', { name: 'Toggle navigation' })).toHaveAttribute(
    'aria-expanded',
    'false',
  );
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const response = await page.request.post('/api/v1/capture', {
    headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
    data: { list_id: 'inbox', tasks: [{ title: `Mobile navigation ${randomUUID()}` }] },
  });
  expect(response.ok()).toBeTruthy();
  const [task] = await response.json();
  await open(page, '/inbox');
  await page.getByRole('link', { name: task.title, exact: true }).click();
  await expect(page.locator('main[data-task-page]')).toBeVisible();
  await expect(page).toHaveURL(/\/tasks\//);
  await page.goBack();
  await expect(page.locator('h1')).toHaveText('Inbox');
  await persistent(page);
});

test('ordinary forms submit once, keep CSRF and follow the server redirect in place', async ({
  page,
}) => {
  await signin(page);
  await open(page, '/spaces');
  await page.locator('#main [data-workspace-create-open]').click();
  const title = `Form navigation ${randomUUID()}`;
  await page.locator('#workspace-title').fill(title);
  let saves = 0;
  page.on('request', (request) => {
    if (request.method() === 'POST' && new URL(request.url()).pathname === '/spaces')
      saves++;
  });
  await page.getByRole('button', { name: 'Create space', exact: true }).click();
  await expect(page).toHaveURL(/\/space\//);
  await expect(page.locator('h1')).toContainText(title);
  expect(saves).toBe(1);
  await persistent(page);

  await open(page, '/spaces');
  await page.locator('#main [data-workspace-create-open]').click();
  await page.locator('#workspace-title').fill('Keep an unconfirmed draft');
  await page.route('**/spaces', (route) =>
    route.request().method() === 'POST'
      ? route.fulfill({ status: 503, body: 'Unavailable' })
      : route.continue(),
  );
  await page.getByRole('button', { name: 'Create space', exact: true }).click();
  await expect(page.locator('.navigation-error')).toContainText(
    'save could not be confirmed',
  );
  await expect(page.locator('#workspace-title')).toHaveValue('Keep an unconfirmed draft');
  await expect(
    page.getByRole('button', { name: 'Create space', exact: true }),
  ).toBeEnabled();
  expect(saves).toBe(2);
  await persistent(page);
});

test('cached lists open through HTMX offline and uncached screens keep the current page', async ({
  page,
  context,
  browserName,
}) => {
  // Playwright's service-worker network emulation is supported on Chromium.
  // https://playwright.dev/docs/service-workers
  test.skip(
    browserName !== 'chromium',
    'Service-worker offline emulation requires Chromium',
  );
  await signin(page);
  const listURL = page.url();
  await workerReady(page);
  await open(page, listURL);
  await expect
    .poll(() =>
      page.evaluate(async (url) => {
        for (const key of await caches.keys()) {
          if (
            key.startsWith('todoer-page-') &&
            (await (await caches.open(key)).match(url))
          )
            return true;
        }
        return false;
      }, listURL),
    )
    .toBe(true);
  await open(page, '/settings');
  await context.setOffline(true);
  await open(page, listURL);
  await expect(page).toHaveURL(listURL);
  await expect(page.locator('#task-list')).toBeVisible();
  await open(page, '/today');
  await expect(page).toHaveURL(listURL);
  await expect(page.locator('.navigation-error')).toBeVisible();
  await persistent(page);
});

test('Back restores page scroll and forward navigation starts at the top', async ({
  page,
}) => {
  await signin(page);
  await page.evaluate(() => {
    const sheet = [...document.styleSheets].find(
      (sheet) => sheet.href && new URL(sheet.href).origin === location.origin,
    );
    sheet.insertRule('#main { min-height: 2400px; }', sheet.cssRules.length);
  });
  await page.evaluate(() => scrollTo(0, 750));
  await expect.poll(() => page.evaluate(() => Math.abs(scrollY - 750))).toBeLessThan(1);
  await open(page, '/today');
  await expect.poll(() => page.evaluate(() => scrollY)).toBe(0);
  await page.goBack();
  await expect(page.locator('#task-list')).toBeVisible();
  await expect.poll(() => page.evaluate(() => Math.abs(scrollY - 750))).toBeLessThan(1);
  await persistent(page);
});

test('expired authentication keeps the current capture draft and offers sign-in', async ({
  page,
  context,
}) => {
  await signin(page);
  const listURL = page.url();
  await page.locator('#capture-open').click();
  await page.locator('#capture-title').fill('Draft across an expired session');
  await context.clearCookies();
  await open(page, '/today');
  await expect(page).toHaveURL(listURL);
  await expect(page.locator('#capture-title')).toHaveValue(
    'Draft across an expired session',
  );
  await expect(page.locator('.navigation-error')).toContainText('Your session ended');
  await expect(page.locator('.navigation-error a')).toHaveAttribute(
    'href',
    /\/signin\?next=/,
  );
  await persistent(page);
});

test('quick capture keeps its draft and uses the new list after navigation', async ({
  page,
}) => {
  await signin(page);
  await page.evaluate(async () =>
    (await import('/assets/quick-capture.js')).openCapture(),
  );
  const capture = page.locator('.capture-omnibar');
  await capture.locator('[name=title]').fill('Quick capture navigation draft');
  await capture.getByRole('button', { name: 'Close capture' }).click();
  await workerReady(page);
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const headers = { 'x-csrf-token': csrf };
  const space = await (
    await page.request.post('/api/v1/workspaces', {
      headers,
      data: { title: `Quick capture ${randomUUID()}` },
    })
  ).json();
  const list = await (
    await page.request.post('/api/v1/lists', {
      headers,
      data: { title: `Destination ${randomUUID()}`, workspace_id: space.id },
    })
  ).json();
  await open(page, `/lists/${list.id}`);
  await page.evaluate(async () =>
    (await import('/assets/quick-capture.js')).openCapture(),
  );
  await expect(capture.locator('[name=title]')).toHaveValue(
    'Quick capture navigation draft',
  );
  await expect(capture.locator('[name=list_id]')).toHaveValue(list.id);
  const written = page.waitForResponse(
    (response) =>
      response.url().endsWith('/api/v1/capture') &&
      response.request().method() === 'POST',
  );
  await capture.getByRole('button', { name: 'Add task', exact: true }).click();
  const [task] = await (await written).json();
  expect(task.list_id).toBe(list.id);
  await persistent(page);
});
