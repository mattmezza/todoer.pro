import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('space list cards reorder and persist', async ({ page, browserName }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
  const post = async (path, data) => {
    const response = await page.request.post(path, {
      headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
      data,
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    return response.json();
  };
  const space = await post('/api/v1/workspaces', { title: `Ordering ${randomUUID()}` });
  const a = await post('/api/v1/lists', { workspace_id: space.id, title: 'First list' });
  const b = await post('/api/v1/lists', { workspace_id: space.id, title: 'Second list' });
  await page.goto(`/space/${space.id}`);
  const grid = page.locator('[data-list-order-enabled]');
  const first = grid.locator(`[data-list-order-id="${a.id}"]`);
  const second = grid.locator(`[data-list-order-id="${b.id}"]`);
  if (browserName === 'chromium') {
    await grid.evaluate((element) => element.scrollIntoView({ block: 'center' }));
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Emulation.setTouchEmulationEnabled', { enabled: true });
    const from = await first.boundingBox(),
      to = await second.boundingBox();
    await cdp.send('Input.dispatchTouchEvent', {
      type: 'touchStart',
      touchPoints: [{ x: from.x + 30, y: from.y + 30 }],
    });
    await page.waitForTimeout(500);
    const saved = page.waitForResponse((response) =>
      response.url().endsWith('/reorder-visible'),
    );
    await cdp.send('Input.dispatchTouchEvent', {
      type: 'touchMove',
      touchPoints: [{ x: to.x + 30, y: to.y + to.height - 10 }],
    });
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    expect((await saved).status()).toBe(200);
    await cdp.send('Emulation.setTouchEmulationEnabled', { enabled: false });
  } else {
    await first.locator('a').focus();
    const saved = page.waitForResponse((response) =>
      response.url().endsWith('/reorder-visible'),
    );
    await page.keyboard.press('Alt+ArrowDown');
    expect((await saved).status()).toBe(200);
  }
  await expect(grid.locator('[data-list-order-id]').first()).toHaveAttribute(
    'data-list-order-id',
    b.id,
  );
  await page.reload();
  await expect(grid.locator('[data-list-order-id]').first()).toHaveAttribute(
    'data-list-order-id',
    b.id,
  );
  await page.setViewportSize({ width: 1280, height: 900 });
  const desktopSaved = page.waitForResponse((response) =>
    response.url().endsWith('/reorder-visible'),
  );
  const target = await first.boundingBox();
  await second.dragTo(first, {
    targetPosition: { x: target.width * 0.85, y: target.height / 2 },
  });
  expect((await desktopSaved).status()).toBe(200);
  await expect(grid.locator('[data-list-order-id]').first()).toHaveAttribute(
    'data-list-order-id',
    a.id,
  );
  const add = grid.getByRole('button', { name: 'Add list' });
  await add.click();
  const dialog = page.locator('#list-create-dialog');
  await expect(dialog).toBeVisible();
  await expect(dialog.locator('[name="workspace_id"]')).toHaveValue(space.id);
  await dialog.getByRole('button', { name: 'Close new list' }).click();
  await page.route('**/lists/reorder-visible', (route) =>
    route.fulfill({ status: 409, json: { error: { message: 'Stale order' } } }),
  );
  await first.locator('a').focus();
  await page.keyboard.press('Alt+ArrowDown');
  await expect(page.getByText('Stale order', { exact: false })).toBeVisible();
  await expect(add).toBeVisible();
  await expect(grid.locator('[data-list-order-id]').first()).toHaveAttribute(
    'data-list-order-id',
    a.id,
  );
});
