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
  expect(box.width).toBeGreaterThanOrEqual(36);
  async function gesture(id, delta, hold = 0, endY = 0, duringSwipe = null) {
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
    if (duringSwipe) await duringSwipe();
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  }
  async function checkReveal(id, action, label) {
    await expect(row(id)).toHaveAttribute('data-swipe-action', action);
    expect(await row(id).evaluate((el) => getComputedStyle(el, '::before').content)).toBe(
      `"${label}"`,
    );
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    ).toBeTruthy();
    if (action === 'done') {
      const gap = await row(id).evaluate((el) => {
        const rowBounds = el.getBoundingClientRect();
        const control = el.querySelector('.completion').getBoundingClientRect();
        const circleWidth = parseFloat(
          getComputedStyle(el.querySelector('.completion'), '::before').width,
        );
        return control.left + (control.width - circleWidth) / 2 - (rowBounds.left + 120);
      });
      expect(gap).toBeGreaterThanOrEqual(8);
    }
  }
  await gesture(tasks[0].id, 120, 0, 0, () => checkReveal(tasks[0].id, 'done', 'Done'));
  await expect(row(tasks[0].id)).toHaveClass(/is-complete/);
  await expect(row(tasks[1].id)).not.toHaveClass(/is-complete/);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(row(tasks[0].id)).not.toHaveClass(/is-complete/);
  await gesture(tasks[1].id, -120, 0, 0, () =>
    checkReveal(tasks[1].id, 'archive', 'Archive'),
  );
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

for (const surface of ['panel', 'page']) {
  test(`subtask swipes complete, archive and undo in the task ${surface}`, async ({
    page,
    browserName,
  }) => {
    test.skip(browserName !== 'chromium', 'Uses Chromium touch input protocol');
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto('/signin');
    await page.getByRole('button', { name: 'Enter development workspace' }).click();
    const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
    const capture = async (title, parent_id = null) => {
      const response = await page.request.post('/api/v1/capture', {
        headers: { 'X-CSRF-Token': csrf, 'Idempotency-Key': randomUUID() },
        data: { list_id: 'inbox', tasks: [{ title, parent_id }] },
      });
      expect(response.ok()).toBeTruthy();
      return (await response.json())[0];
    };
    const parent = await capture(`Swipe subtasks ${randomUUID()}`);
    const first = await capture('Swipe first child', parent.id);
    const second = await capture('Swipe second child', parent.id);
    if (surface === 'panel') {
      await page.goto('/lists/inbox');
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
      await page
        .locator(`.task[data-id="${parent.id}"]`)
        .click({ position: { x: 16, y: 6 } });
    } else await page.goto(`/tasks/${parent.id}`);
    const root = page.locator('[data-task-page]');
    const row = (task) => root.locator(`[data-child-id="${task.id}"]`);
    await expect(row(first).locator('.task-child-complete')).toBeEnabled();
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Emulation.setTouchEmulationEnabled', { enabled: true });
    async function swipe(task, delta, action) {
      await row(task).scrollIntoViewIfNeeded();
      const bounds = await row(task).boundingBox();
      const x = bounds.x + Math.min(100, bounds.width / 2),
        y = bounds.y + bounds.height / 2;
      await cdp.send('Input.dispatchTouchEvent', {
        type: 'touchStart',
        touchPoints: [{ x, y }],
      });
      await cdp.send('Input.dispatchTouchEvent', {
        type: 'touchMove',
        touchPoints: [{ x: x + delta, y }],
      });
      await expect(row(task)).toHaveAttribute('data-swipe-action', action);
      expect(
        await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
      ).toBe(true);
      await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    }
    await swipe(first, 120, 'done');
    await expect(row(first).locator('.task-child-complete')).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    await expect(row(second).locator('.task-child-complete')).toHaveAttribute(
      'aria-pressed',
      'false',
    );
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    await expect(row(first).locator('.task-child-complete')).toHaveAttribute(
      'aria-pressed',
      'false',
    );
    await swipe(second, -120, 'archive');
    await expect(row(second)).toHaveCount(0);
    await expect(row(first)).toBeVisible();
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    await expect(row(second)).toBeVisible();
    await page.locator('#navigation').evaluate((el) => {
      el.dataset.swipeRight = 'none';
    });
    await swipe(first, 120, 'none');
    await expect(row(first).locator('.task-child-complete')).toHaveAttribute(
      'aria-pressed',
      'false',
    );
    await expect(page.getByRole('button', { name: 'Undo', exact: true })).toHaveCount(0);
    await expect(root).toHaveAttribute('data-task-id', parent.id);
  });
}
