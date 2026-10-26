import http from 'node:http';
import { rm } from 'node:fs/promises';
import { test, expect } from '@playwright/test';
const auth = { Authorization: 'Bearer local-integration-token-not-for-production' };
async function signin(page) {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await expect(page.locator('h1')).toHaveText('#Groceries');
}
test('capture, optimistic completion, edit conflict and draft retention', async ({
  page,
  request,
}) => {
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await signin(page);
  const title = `Browser task ${crypto.randomUUID()}`;
  await page.locator('#capture-title').fill(title);
  await page.locator('#capture-form button').click();
  const row = page
    .locator('.task')
    .filter({ has: page.getByRole('button', { name: title, exact: true }) });
  await expect(row).toHaveCount(1);
  await row.locator('.completion').click();
  await expect(row).toHaveAttribute('data-complete', 'true');
  await row.locator('.completion').click();
  await expect(row).toHaveAttribute('data-complete', 'false');
  await row.locator('.task-title').click();
  await page.locator('#edit-title').fill('Human unsaved draft');
  const id = await row.getAttribute('data-id');
  const before = await request.get(`/api/v1/tasks/${id}/subtree`, { headers: auth });
  const [task] = await before.json();
  const update = await request.patch(`/api/v1/tasks/${id}`, {
    headers: auth,
    data: { revision: task.revision, title: 'Agent concurrent edit' },
  });
  expect(update.status()).toBe(200);
  await page.waitForTimeout(2500);
  await expect(page.locator('#edit-title')).toHaveValue('Human unsaved draft');
  await page.locator('#edit-form button[type=submit]').click();
  await expect(page.locator('#edit-error')).toContainText('stale revision');
  await expect(page.locator('#edit-title')).toHaveValue('Human unsaved draft');
  await page.reload();
  await expect(page.locator('#task-editor')).toBeVisible();
  await expect(page.locator('#edit-title')).toHaveValue('Human unsaved draft');
  expect(errors).toEqual([]);
});
for (const [name, width, height] of [
  ['phone', 390, 844],
  ['tablet', 820, 1180],
  ['desktop', 1440, 1000],
]) {
  test(`${name}: navigation, touch targets and no overflow`, async ({ page }) => {
    await page.setViewportSize({ width, height });
    await signin(page);
    await page.locator('#capture-title').blur();
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    ).toBe(true);
    const completion = page.locator('.completion').first();
    const box = await completion.boundingBox();
    expect(Math.round(box.width)).toBeGreaterThanOrEqual(44);
    expect(Math.round(box.height)).toBeGreaterThanOrEqual(44);
    if (width < 640) {
      await page.getByRole('button', { name: 'Toggle navigation' }).click();
      await expect(page.locator('#navigation')).toBeVisible();
      await page
        .locator('#navigation')
        .getByRole('link', { name: 'Inbox', exact: true })
        .first()
        .click();
      await expect(page.locator('h1')).toHaveText('#Inbox');
    }
    await page.screenshot({
      path: `.local/rebuild/rust-${name}-${test.info().project.name}.png`,
      fullPage: true,
    });
  });
}
test('light theme and reduced motion remain usable', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'light', reducedMotion: 'reduce' });
  await signin(page);
  expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe(
    'rgb(255, 255, 255)',
  );
  await page.screenshot({
    path: `.local/rebuild/rust-light-${test.info().project.name}.png`,
    fullPage: true,
  });
});
test('nested checklist opens deeper levels without squeezed indentation', async ({
  page,
  request,
}) => {
  const rootResponse = await request.post('/api/v1/capture', {
    headers: { ...auth, 'Idempotency-Key': crypto.randomUUID() },
    data: { list_id: 'groceries', tasks: [{ title: `Project ${crypto.randomUUID()}` }] },
  });
  const [root] = await rootResponse.json();
  await signin(page);
  await page.goto(`/lists/groceries?task=${root.id}`);
  await expect(page.locator('#task-editor')).toBeVisible();
  await page.locator('#subtask-title').fill('First step');
  await page.locator('#add-subtask').click();
  await expect(page.locator('#subtask-list')).toContainText('First step');
  await expect(page.locator('#subtask-progress')).toHaveText('0/1');
  await page.locator('.subtask-open').click();
  await expect(page.locator('#edit-title')).toHaveValue('First step');
  await page.locator('#subtask-title').fill('Nested step');
  await page.locator('#add-subtask').click();
  await expect(page.locator('#subtask-list')).toContainText('Nested step');
  await page.goBack();
  await expect(page.locator('#edit-title')).toHaveValue(root.title);
  await expect(page.locator('#subtask-progress')).toHaveText('0/1');
});
test('offline capture survives reload and syncs exactly once', async ({
  page,
  context,
  request,
}) => {
  await signin(page);
  await page.evaluate(async () => {
    await navigator.serviceWorker.ready;
  });
  await page.waitForFunction(() => navigator.serviceWorker.controller !== null);
  await page.waitForTimeout(1000);
  const title = `Offline capture ${crypto.randomUUID()}`;
  await context.setOffline(true);
  await page.locator('#capture-title').fill(title);
  await page.locator('#capture-form button').click();
  await expect(page.locator('#pending-captures')).toContainText(title);
  await page.reload();
  await expect(page.locator('h1')).toHaveText('#Groceries');
  await expect(page.locator('#pending-captures')).toContainText(title);
  await context.setOffline(false);
  await expect(page.locator('#pending-captures')).not.toContainText(title, {
    timeout: 15000,
  });
  const response = await request.get(
    `/api/v1/search?query=${encodeURIComponent(title)}`,
    { headers: auth },
  );
  const tasks = await response.json();
  expect(tasks.filter((t) => t.title === title)).toHaveLength(1);
});
test('search, trash, restore, activity and guarded undo use real operations', async ({
  page,
  request,
}) => {
  const title = `Recovery ${crypto.randomUUID()}`;
  const response = await request.post('/api/v1/capture', {
    headers: { ...auth, 'Idempotency-Key': crypto.randomUUID() },
    data: {
      list_id: 'groceries',
      tasks: [{ title, description: '**Safe brief** <script>alert(1)</script>' }],
    },
  });
  const [task] = await response.json();
  await signin(page);
  await page.goto(`/lists/groceries?task=${task.id}`);
  await page.locator('#edit-title').fill(`${title} updated`);
  await page.locator('#edit-form button[type=submit]').click();
  await expect(page.locator('#notice')).toContainText('Changes saved');
  await expect(page.locator('#task-editor')).not.toBeVisible();
  await page.goto('/activity');
  const activity = page.locator('article').filter({ hasText: `${title} updated` });
  await activity.getByRole('button', { name: 'Undo edit' }).first().click();
  await expect(page.locator('#notice')).toContainText('Edit undone');
  await page.goto(`/search?query=${encodeURIComponent(title)}`);
  await expect(page.locator('article').filter({ hasText: title })).toHaveCount(1);
  await expect(
    page.locator('.markdown strong').filter({ hasText: 'Safe brief' }),
  ).toBeVisible();
  expect(await page.locator('main script').count()).toBe(0);
  await page.goto(`/lists/groceries?task=${task.id}`);
  page.once('dialog', (d) => d.accept());
  await page.getByRole('button', { name: 'Move to trash' }).click();
  await expect(page.locator('#notice')).toContainText('moved to trash');
  await page.goto('/trash');
  const row = page.locator('article').filter({ hasText: title });
  await row.getByRole('button', { name: 'Restore' }).click();
  await expect(page.locator('#notice')).toContainText('Task restored');
});
test('durable capture survives an actual browser restart', async ({
  browser,
  request,
}, testInfo) => {
  test.setTimeout(100000);
  test.skip(
    testInfo.project.name !== 'chromium',
    'Persistent-profile restart exercised in Chromium; Firefox reload covered separately',
  );
  // Playwright's page offline emulation does not reliably block a restarted
  // service worker's fetch. An isolated loopback proxy supplies a real outage.
  let online = true;
  const proxy = http.createServer((req, res) => {
    if (!online) {
      req.socket.destroy();
      return;
    }
    const headers = { ...req.headers };
    if (headers.origin) headers.origin = 'http://127.0.0.1:8092';
    const upstream = http.request(
      { hostname: '127.0.0.1', port: 8092, path: req.url, method: req.method, headers },
      (response) => {
        res.writeHead(response.statusCode, response.headers);
        response.pipe(res);
      },
    );
    upstream.on('error', () => res.destroy());
    req.pipe(upstream);
  });
  await new Promise((resolve) => proxy.listen(0, '127.0.0.1', resolve));
  const origin = `http://127.0.0.1:${proxy.address().port}`;
  const profile = `.local/rebuild/profile-${crypto.randomUUID()}`;
  let context;
  try {
    context = await browser
      .browserType()
      .launchPersistentContext(profile, { headless: true });
    let page = await context.newPage();
    await page.goto(`${origin}/signin`);
    await page.getByRole('button', { name: 'Enter development workspace' }).click();
    await page.waitForFunction(() => navigator.serviceWorker.controller !== null);
    await page.waitForTimeout(1000);
    const title = `Restart capture ${crypto.randomUUID()}`;
    online = false;
    await page.locator('#capture-title').fill(title);
    await page.locator('#capture-form button').click();
    await expect(page.locator('#pending-captures')).toContainText(title);
    await context.close();
    context = await browser
      .browserType()
      .launchPersistentContext(profile, { headless: true });
    page = await context.newPage();
    await page.goto(`${origin}/lists/groceries`);
    await expect(page.locator('#pending-captures')).toContainText(title);
    online = true;
    await page.locator('#retry-captures').click();
    await expect(page.locator('#pending-captures')).not.toContainText(title, {
      timeout: 80000,
    });
    const response = await request.get(
      `/api/v1/search?query=${encodeURIComponent(title)}`,
      { headers: auth },
    );
    expect((await response.json()).filter((t) => t.title === title)).toHaveLength(1);
  } finally {
    await context?.close();
    proxy.closeAllConnections();
    await new Promise((resolve) => proxy.close(resolve));
    await rm(profile, { recursive: true, force: true });
  }
});
