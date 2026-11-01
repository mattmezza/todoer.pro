import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('space favorites appear first and rows reorder by touch', async ({
  page,
  browserName,
}) => {
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
  const space = await post('/api/v1/workspaces', {
    title: `Blue Mountain ${randomUUID()}`,
  });
  const list = await post('/api/v1/lists', {
    workspace_id: space.id,
    title: 'Favorite list',
  });
  const set = await page.request.put('/api/v1/favorites', {
    headers: { 'x-csrf-token': csrf },
    data: { kind: 'list', id: list.id, favorite: true },
  });
  expect(set.ok()).toBeTruthy();
  await page.goto(`/space/${space.id}`);
  await page.locator('.space-overview-heading .entity-menu summary').click();
  await page.locator('[data-space-favorite]').click();
  await page.getByRole('button', { name: 'Toggle navigation' }).click();
  const nav = page.locator('[data-favorite-order]');
  const spaceRow = nav.locator(`[data-favorite-id="${space.id}"]`);
  const listRow = nav.locator(`[data-favorite-id="${list.id}"]`);
  await expect(spaceRow).toBeVisible();
  await expect(spaceRow.locator('.favorite-space-avatar')).toHaveText('BM');
  await expect(nav.locator('[data-favorite-id]').first()).toHaveAttribute(
    'data-favorite-id',
    space.id,
  );
  if (browserName === 'chromium') {
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Emulation.setTouchEmulationEnabled', { enabled: true });
    const from = await spaceRow.boundingBox(),
      to = await listRow.boundingBox();
    const x = from.x + from.width / 2,
      y = from.y + from.height / 2;
    await cdp.send('Input.dispatchTouchEvent', {
      type: 'touchStart',
      touchPoints: [{ x, y }],
    });
    await page.waitForTimeout(500);
    const saved = page.waitForResponse(
      (response) =>
        response.url().endsWith('/favorites/order') &&
        response.request().method() === 'PUT',
    );
    await cdp.send('Input.dispatchTouchEvent', {
      type: 'touchMove',
      touchPoints: [{ x: to.x + to.width / 2, y: to.y + to.height - 3 }],
    });
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    expect((await saved).status()).toBe(200);
    const relativeOrder = async () => {
      const ids = await page
        .locator('[data-favorite-order] [data-favorite-id]')
        .evaluateAll((rows) => rows.map((row) => row.dataset.favoriteId));
      return ids.indexOf(space.id) - ids.indexOf(list.id);
    };
    await expect.poll(relativeOrder).toBe(1);
    await page.reload();
    await page.getByRole('button', { name: 'Toggle navigation' }).click();
    await expect.poll(relativeOrder).toBe(1);
  }
});
