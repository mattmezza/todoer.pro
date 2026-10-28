import { test, expect } from '@playwright/test';
async function login(page) {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
}
function gate() {
  let release;
  const wait = new Promise((resolve) => {
    release = resolve;
  });
  return { wait, release };
}
test('settings conceal unconfirmed values and retain their geometry during slow loads', async ({
  page,
}) => {
  await login(page);
  const delayed = gate();
  await page.route('**/api/v1/notification-preferences', async (route) => {
    await delayed.wait;
    await route.continue();
  });
  await page.goto('/settings');
  const form = page.locator('[data-calendar-preferences]');
  await expect(form).toHaveAttribute('data-loading-pending', 'form');
  await expect(form.locator('[name=timezone]')).toBeHidden();
  expect(await form.evaluate((el) => el.inert)).toBe(true);
  const height = (await form.boundingBox()).height;
  delayed.release();
  await expect(form).not.toHaveAttribute('data-loading-pending');
  await expect(form.locator('[name=timezone]')).toBeVisible();
  expect(Math.abs((await form.boundingBox()).height - height)).toBeLessThan(12);
  await expect(form.locator('button[type=submit]')).toBeEnabled();
});
test('failed initial loads stop shimmering, offer retry, and never expose editable defaults', async ({
  page,
}) => {
  await login(page);
  let fail = true;
  await page.route('**/api/v1/capture/preferences', (route) =>
    fail
      ? route.fulfill({ status: 503, contentType: 'application/json', body: '{}' })
      : route.continue(),
  );
  await page.goto('/settings/ai');
  const form = page.locator('[data-capture-preferences]');
  await expect(form.locator('.loading-error')).toBeVisible();
  await expect(form).not.toHaveAttribute('data-loading-pending');
  await expect(form.locator('[name=smart_enabled]')).toBeHidden();
  fail = false;
  await form.locator('.loading-error button').click();
  await expect(form.locator('.loading-error')).toHaveCount(0);
  await expect(form.locator('[name=smart_enabled]')).toBeVisible();
  await expect(form.locator('button[type=submit]')).toBeEnabled();
});
test('inbox uses a skeleton initially and retains loaded content and actions during refresh', async ({
  page,
}) => {
  await login(page);
  let delayed = gate();
  await page.route('**/api/v1/inbox', async (route) => {
    await delayed.wait;
    await route.continue();
  });
  await page.goto('/inbox');
  const content = page.locator('[data-workbench-content]');
  await expect(content).toHaveAttribute('data-loading-pending', 'rows');
  delayed.release();
  await expect(content).toHaveAttribute('data-loaded', 'true');
  const html = await content.innerHTML();
  delayed = gate();
  await page.evaluate(() =>
    document.dispatchEvent(new Event('todoer:workbench-refresh')),
  );
  await expect(content).toHaveAttribute('aria-busy', 'true');
  await expect(content).not.toHaveAttribute('data-loading-pending');
  expect(await content.evaluate((el) => el.inert)).toBe(false);
  expect(await content.innerHTML()).toBe(html);
  delayed.release();
  await expect(content).not.toHaveAttribute('aria-busy');
});
test('loading ownership, retry and reduced motion work across viewport sizes', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.evaluate(async () => {
    const { beginLoading } = await import('/assets/loading.js');
    const target = document.createElement('section');
    target.id = 'loading-test';
    target.innerHTML = '<button>Real action</button>';
    document.querySelector('main').append(target);
    const old = beginLoading(target);
    const current = beginLoading(target, { variant: 'form' });
    old.finish();
    window.finishLoadingTest = current.finish;
  });
  const target = page.locator('#loading-test');
  await expect(target).toHaveAttribute('data-loading-pending', 'form');
  await page.emulateMedia({ reducedMotion: 'reduce' });
  expect(
    await target.evaluate(
      (el) => getComputedStyle(el.querySelector('.skeleton-line')).animationName,
    ),
  ).toBe('none');
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBe(true);
  await page.evaluate(() => window.finishLoadingTest());
  await expect(target.getByRole('button')).toBeVisible();
  expect(await target.evaluate((el) => el.inert)).toBe(false);
});

test('notifications show skeletons until a confirmed empty response arrives', async ({
  page,
}) => {
  await login(page);
  const delayed = gate();
  await page.route('**/api/v1/notifications?*', async (route) => {
    await delayed.wait;
    await route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify({ items: [], unread: 0, latest_id: 0, next_cursor: null }),
    });
  });
  await page.goto('/notifications');
  const items = page.locator('[data-notification-items]');
  await expect(items).toHaveAttribute('data-loading-pending', 'rows');
  await expect(items.getByText('No notifications yet.')).toHaveCount(0);
  await page.screenshot({ path: '.local/rebuild/notifications-skeleton.png' });
  delayed.release();
  await expect(items).not.toHaveAttribute('data-loading-pending');
  await expect(items.getByText('No notifications yet.')).toBeVisible();
});

test('quick capture can be dismissed during a slow load without reopening later', async ({
  page,
}) => {
  await login(page);
  const delayed = gate();
  await page.route(/\/capture(?:\?.*)?$/, async (route) => {
    await delayed.wait;
    await route.continue();
  });
  await page.evaluate(() => {
    import('/assets/quick-capture.js').then((module) => module.openCapture());
  });
  const pending = page.getByRole('dialog', { name: 'Loading quick capture' });
  await expect(pending).toBeVisible();
  await expect(pending.locator('[data-loading-pending]')).toBeVisible();
  await pending.getByRole('button', { name: 'Close', exact: true }).click();
  delayed.release();
  await expect(page.locator('.capture-omnibar')).toHaveCount(0);
  await page.evaluate(() => {
    import('/assets/quick-capture.js').then((module) => module.openCapture());
  });
  await expect(page.locator('.capture-omnibar input[name=title]')).toBeVisible();
});

test('search displays a visible skeleton for an empty results container and ignores stale responses', async ({
  page,
}) => {
  await login(page);
  const delayed = gate();
  await page.route('**/api/v1/query?*', async (route) => {
    await delayed.wait;
    await route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify({ hits: [], total: 0 }),
    });
  });
  await page.evaluate(async () =>
    (await import('/assets/search-omnibar.js')).openSearch(),
  );
  const input = page.getByRole('combobox', { name: 'Search everything' });
  await input.fill('test');
  const results = page.locator('#omnibar-results');
  await expect(results).toHaveAttribute('data-loading-pending', 'rows');
  await expect(results).toBeVisible();
  await input.fill('');
  await expect(results).not.toHaveAttribute('data-loading-pending');
  delayed.release();
  await expect(page.locator('.omnibar-status')).toContainText('Search tasks, spaces');
});

test('planning and activity panels use placeholders while preserving task content', async ({
  page,
}) => {
  await login(page);
  await page.goto('/lists/inbox');
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const response = await page.request.post('/api/v1/capture', {
    headers: { 'X-CSRF-Token': csrf, 'Idempotency-Key': crypto.randomUUID() },
    data: {
      list_id: 'inbox',
      tasks: [{ title: 'Skeleton panel test', description: '' }],
    },
  });
  expect(response.ok()).toBe(true);
  const [task] = await response.json();
  const delayed = gate();
  await page.route('**/api/v1/tasks/*/schedule', async (route) => {
    await delayed.wait;
    await route.continue();
  });
  await page.goto(`/tasks/${task.id}`);
  await page.evaluate(async () => {
    const root = document.querySelector('[data-task-page]');
    (await import('/assets/list-settings.js')).openTaskSettings(root);
  });
  const panel = page.locator('.list-settings-panel');
  const scheduling = panel.locator('.checklist[data-scheduling]');
  await expect(scheduling).toHaveAttribute('data-loading-pending', 'planning');
  await expect(scheduling.locator('[name=planned_date]')).toBeHidden();
  await page.screenshot({ path: '.local/rebuild/planning-panel-skeleton.png' });
  delayed.release();
  await expect(scheduling).not.toHaveAttribute('data-loading-pending');
  await expect(scheduling.getByText('Set planned date', { exact: true })).toBeVisible();
  const history = gate();
  await page.route('**/api/v1/tasks/*/activity*', async (route) => {
    await history.wait;
    await route.continue();
  });
  await panel.getByRole('tab', { name: 'Activity' }).click();
  await expect(panel.locator('.activity-timeline')).toHaveAttribute(
    'data-loading-pending',
    'activity',
  );
  history.release();
  await expect(panel.locator('.activity-timeline')).not.toHaveAttribute(
    'data-loading-pending',
  );
});
