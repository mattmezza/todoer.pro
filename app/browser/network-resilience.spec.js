import { test, expect } from '@playwright/test';
async function signin(page) {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
}
test('search discards old results immediately and shows contextual empty icon', async ({
  page,
}) => {
  await signin(page);
  await page.goto('/today');
  await page.route('**/api/v1/query?**', (route) => {
    const text = new URL(route.request().url()).searchParams.get('text');
    return route.fulfill({
      json:
        text === 'first'
          ? { total: 1, hits: [{ id: 'fake', kind: 'task', title: 'Old result' }] }
          : { total: 0, hits: [] },
    });
  });
  await page.evaluate(async () =>
    (await import('/assets/search-omnibar.js')).openSearch(),
  );
  const input = page.locator('#omnibar-query');
  await input.fill('first');
  await expect(page.locator('#omnibar-results')).toContainText('Old result');
  await input.fill('second');
  await expect(page.locator('#omnibar-results')).toBeEmpty();
  await expect(page.locator('.omnibar-status')).toContainText('No matches');
  await expect(page.locator('.omnibar-status use')).toHaveAttribute(
    'href',
    '/assets/icons.svg#search',
  );
  expect(await input.evaluate((el) => getComputedStyle(el).textAlign)).toBe('left');
});
test('Inbox queues failed captures and retries refresh', async ({ page }) => {
  await signin(page);
  let calls = 0;
  await page.route('**/api/v1/capture', async (route) => {
    calls++;
    await new Promise((resolve) => setTimeout(resolve, 500));
    await route.abort('failed');
  });
  await page.goto('/inbox');
  await page.getByRole('link', { name: 'Capture a task…' }).click();
  const dialog = page.getByRole('dialog', { name: 'What’s on your mind?' });
  await dialog.getByLabel('Task', { exact: true }).fill('Preserve this draft');
  await dialog.getByRole('button', { name: 'Add task', exact: true }).click();
  await expect(
    page.getByText('Task kept on this device. Open Inbox to retry syncing.'),
  ).toBeVisible();
  expect(calls).toBe(1);
  await page.route('**/api/v1/inbox', (route) => route.abort('failed'));
  await page.evaluate(() =>
    document.dispatchEvent(new Event('todoer:workbench-refresh')),
  );
  await expect(
    page.getByRole('button', { name: 'Try again', exact: true }),
  ).toBeVisible();
  await page.unroute('**/api/v1/inbox');
  await page.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Try again', exact: true })).toHaveCount(
    0,
  );
});
test('network helper never retries an uncertain write and preserves cancellation', async ({
  page,
}) => {
  await page.goto('/signin');
  const result = await page.evaluate(async () => {
    const { request } = await import('/assets/request.js');
    const original = globalThis.fetch;
    let calls = 0;
    globalThis.fetch = async () => {
      calls++;
      throw new TypeError('Failed to fetch');
    };
    let message;
    try {
      await request('/api/v1/capture', { method: 'POST' });
    } catch (e) {
      message = e.message;
    }
    const controller = new AbortController();
    controller.abort();
    globalThis.fetch = async () => {
      throw new DOMException('Aborted', 'AbortError');
    };
    let cancelled;
    try {
      await request('/api/v1/query', { signal: controller.signal });
    } catch (e) {
      cancelled = e.name;
    }
    globalThis.fetch = original;
    return { calls, message, cancelled };
  });
  expect(result.calls).toBe(1);
  expect(result.message).toContain('server may have received');
  expect(result.cancelled).toBe('AbortError');
});

test('settings serialize competing preference saves without losing either choice', async ({
  page,
}) => {
  await signin(page);
  let saved = {
    timezone: 'UTC',
    week_start: 1,
    fun_animations: true,
    fun_animation: 'tasks',
  };
  const writes = [];
  await page.route('**/api/v1/notification-preferences', async (route) => {
    if (route.request().method() === 'PUT') {
      const value = route.request().postDataJSON();
      writes.push(value);
      await new Promise((resolve) => setTimeout(resolve, 300));
      saved = value;
    }
    await route.fulfill({ json: saved });
  });
  await page.goto('/settings/general');
  const zone = page.locator('[name=timezone]');
  await expect(zone).toBeEnabled();
  await expect(page.locator('[name=fun_animations]')).toBeEnabled();
  await zone.fill('Europe/Zurich');
  await page.getByRole('button', { name: 'Save preferences' }).click();
  await page.locator('[name=fun_animations]').uncheck();
  await expect(page.locator('[name=fun_animations]')).toBeEnabled();
  expect(writes).toHaveLength(2);
  expect(saved.timezone).toBe('Europe/Zurich');
  expect(saved.fun_animations).toBe(false);
});
