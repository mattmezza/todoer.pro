import http from 'node:http';
import { rm } from 'node:fs/promises';
import { test, expect } from '@playwright/test';
const auth = { Authorization: 'Bearer local-integration-token-not-for-production' };
async function signin(page) {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await expect(page.locator('h1')).toHaveText('#Groceries');
  await page.waitForFunction(() => document.body.dataset.ready === 'true');
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
  // Search shows escaped snippets; the task view renders the safe Markdown brief.
  expect(await page.locator('main script').count()).toBe(0);
  await page.goto('/lists/groceries');
  await expect(page.locator(`[data-id="${task.id}"] .markdown strong`)).toHaveText(
    'Safe brief',
  );
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

test('inline rename keeps a concurrent draft and checks its revision', async ({
  page,
  request,
}) => {
  const response = await request.post('/api/v1/capture', {
    headers: { ...auth, 'Idempotency-Key': crypto.randomUUID() },
    data: { list_id: 'groceries', tasks: [{ title: `Inline ${crypto.randomUUID()}` }] },
  });
  const [task] = await response.json();
  await signin(page);
  const row = page.locator(`.task[data-id="${task.id}"]`);
  await row.locator('summary').click();
  await row.getByRole('button', { name: 'Rename inline' }).click();
  await row.getByRole('textbox', { name: 'Task title' }).fill('Unsaved inline draft');
  const result = await request.patch(`/api/v1/tasks/${task.id}`, {
    headers: auth,
    data: { revision: task.revision, title: 'Concurrent inline update' },
  });
  expect(result.status()).toBe(200);
  await page.waitForTimeout(2500);
  await expect(row.getByRole('textbox', { name: 'Task title' })).toHaveValue(
    'Unsaved inline draft',
  );
  await page.reload();
  await expect(row.getByRole('textbox', { name: 'Task title' })).toHaveValue(
    'Unsaved inline draft',
  );
  await row.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('#notice')).toContainText('stale revision');
  await row.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(row.locator('.task-title')).toHaveText('Concurrent inline update');
  await row.locator('summary').click();
  await row.getByRole('button', { name: 'Rename inline' }).click();
  await row.getByRole('textbox', { name: 'Task title' }).fill('Inline saved');
  await row.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('#notice')).toContainText('Title saved');
  await expect(row.locator('.task-title')).toHaveText('Inline saved');
});

test('delayed inline opening and another task save cannot replace the draft', async ({
  page,
  request,
}) => {
  const response = await request.post('/api/v1/capture', {
    headers: { ...auth, 'Idempotency-Key': crypto.randomUUID() },
    data: {
      list_id: 'groceries',
      tasks: [
        { title: `Draft race ${crypto.randomUUID()}` },
        { title: `Other editor ${crypto.randomUUID()}` },
      ],
    },
  });
  const [a, b] = await response.json();
  await signin(page);
  const row = page.locator(`.task[data-id="${a.id}"]`);
  await page.route(`**/api/v1/tasks/${a.id}/subtree`, async (route) => {
    const response = await route.fetch();
    await new Promise((resolve) => setTimeout(resolve, 600));
    await route.fulfill({ response });
  });
  await row.locator('summary').click();
  await row.getByRole('button', { name: 'Rename inline' }).click();
  await page.evaluate(() => window.htmx.trigger('#task-list', 'tasks-refresh'));
  await row.locator('[data-action="rename"]').evaluate((button) => button.click());
  await expect(row.locator('.inline-editor')).toHaveCount(1);
  await row
    .getByRole('textbox', { name: 'Task title' })
    .fill('Draft survives another task edit');
  await page.unroute(`**/api/v1/tasks/${a.id}/subtree`);
  await page.locator(`.task[data-id="${b.id}"] .task-title`).click();
  await page.locator('#edit-title').fill('Other task saved');
  await page.locator('#edit-form button[type=submit]').click();
  await expect(page.locator('#notice')).toContainText('Changes saved');
  await expect(row.getByRole('textbox', { name: 'Task title' })).toHaveValue(
    'Draft survives another task edit',
  );
  await page.route(`**/api/v1/tasks/${a.id}`, async (route) => {
    const response = await route.fetch();
    await new Promise((resolve) => setTimeout(resolve, 600));
    await route.fulfill({ response });
  });
  await row.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(row.getByRole('textbox', { name: 'Task title' })).toBeDisabled();
  await expect(row.getByRole('button', { name: 'Cancel', exact: true })).toBeDisabled();
  await expect(page.locator('#notice')).toContainText('Title saved');
  await expect(row.locator('.task-title')).toHaveText('Draft survives another task edit');
});

test('quick capture has a real native form fallback before scripts load', async ({
  browser,
}) => {
  const context = await browser.newContext({ javaScriptEnabled: false });
  const page = await context.newPage();
  try {
    await page.goto('http://127.0.0.1:8092/signin');
    await page.getByRole('button', { name: 'Enter development workspace' }).click();
    const title = `Native capture ${crypto.randomUUID()}`;
    await page.locator('#capture-title').fill(title);
    await page.locator('#capture-form button').click();
    await expect(page.locator('.task-title').filter({ hasText: title })).toHaveCount(1);
  } finally {
    await context.close();
  }
});

test('live row replacement waits until a pointer click opens details', async ({
  page,
}) => {
  await signin(page);
  const title = page.locator('.task-title').first();
  await title.scrollIntoViewIfNeeded();
  const box = await title.boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  const swapped = await page.evaluate(() => {
    const detail = { target: document.querySelector('#task-list'), shouldSwap: true };
    document.body.dispatchEvent(new CustomEvent('htmx:beforeSwap', { detail }));
    return detail.shouldSwap;
  });
  expect(swapped).toBe(false);
  await page.mouse.up();
  await expect(page.locator('#task-editor')).toBeVisible();
});
