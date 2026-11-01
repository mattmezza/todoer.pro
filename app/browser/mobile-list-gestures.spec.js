import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('mobile row gestures target one task and section controls fit the screen', async ({
  page,
  browserName,
}) => {
  test.skip(browserName !== 'chromium', 'Uses Chromium touch input protocol');
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
  async function post(path, data) {
    const response = await page.request.post(path, {
      headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
      data,
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    return response.json();
  }
  const space = await post('/api/v1/workspaces', { title: `Touch ${randomUUID()}` });
  const list = await post('/api/v1/lists', {
    workspace_id: space.id,
    title: 'Touch list',
  });
  await post(`/api/v1/lists/${list.id}/sections`, { title: 'Section' });
  const tasks = await post('/api/v1/capture', {
    list_id: list.id,
    tasks: [
      { title: 'First touch task' },
      { title: 'Second touch task' },
      { title: 'Third touch task' },
    ],
  });
  await page.goto(`/lists/${list.id}`);
  await page.waitForFunction(() => document.body.dataset.ready === 'true');
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('Emulation.setTouchEmulationEnabled', { enabled: true });
  const row = (id) => page.locator(`.task[data-id="${id}"]`);
  const section = page.locator('.section-collapse > summary').first();
  const box = await section.boundingBox();
  expect(box.x).toBeGreaterThanOrEqual(0);
  expect(box.width).toBeGreaterThanOrEqual(44);
  async function gesture(id, delta, hold = 0, endY = 0) {
    const bounds = await row(id).boundingBox();
    const x = bounds.x + Math.min(100, bounds.width / 2),
      y = bounds.y + bounds.height / 2;
    await cdp.send('Input.dispatchTouchEvent', {
      type: 'touchStart',
      touchPoints: [{ x, y }],
    });
    if (hold) await page.waitForTimeout(hold);
    await cdp.send('Input.dispatchTouchEvent', {
      type: 'touchMove',
      touchPoints: [{ x: x + delta, y: endY || y }],
    });
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  }
  await gesture(tasks[0].id, 120);
  await expect(row(tasks[0].id)).toHaveClass(/is-complete/);
  await expect(row(tasks[1].id)).not.toHaveClass(/is-complete/);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(row(tasks[0].id)).not.toHaveClass(/is-complete/);
  await gesture(tasks[1].id, -120);
  await expect(row(tasks[1].id)).toHaveCount(0);
  await expect(row(tasks[0].id)).toBeVisible();
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(row(tasks[1].id)).toBeVisible();
  const target = await row(tasks[2].id).boundingBox();
  await gesture(tasks[0].id, 0, 550, target.y + target.height - 5);
  await expect
    .poll(() =>
      page
        .locator('.task[data-parent-id=""]')
        .evaluateAll((rows) => rows.map((item) => item.dataset.id)),
    )
    .toEqual([tasks[1].id, tasks[2].id, tasks[0].id]);
});
