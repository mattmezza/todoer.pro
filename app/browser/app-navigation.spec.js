import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

async function signin(page) {
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

test('navigation keeps the shell, URLs, titles and Back/Forward working', async ({
  page,
}) => {
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await signin(page);
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
  await persistent(page);
  expect(errors).toEqual([]);
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
  await page.locator('#capture-title').fill('Still editable while loading');
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
