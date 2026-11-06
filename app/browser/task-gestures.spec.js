import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

async function setup(page) {
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
  const complete = async (id) => {
    const response = await page.request.get(`/api/v1/tasks/${id}/subtree`);
    expect(response.ok()).toBeTruthy();
    return (await response.json()).find((task) => task.id === id).complete;
  };
  return { capture, complete };
}

for (const touch of [false, true]) {
  test.describe(touch ? 'double tap' : 'double click', () => {
    test.use({ hasTouch: touch, viewport: { width: touch ? 390 : 1280, height: 844 } });
    test('completes task rows and titles, and subtask titles in the panel', async ({
      page,
    }) => {
      const { capture, complete } = await setup(page);
      const first = await capture(`Gesture title ${randomUUID()}`);
      const second = await capture(`Gesture row ${randomUUID()}`);
      const parent = await capture(`Gesture parent ${randomUUID()}`);
      const child = await capture('Gesture child', parent.id);
      await page.goto('/lists/inbox');
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
      const originalUrl = page.url();
      const activate = async (locator, options = {}) => {
        if (touch) {
          await locator.tap(options);
          await locator.tap(options);
        } else await locator.dblclick(options);
      };
      const writes = [];
      page.on('request', (request) => {
        if (
          request.method() === 'PATCH' &&
          /\/api\/v1\/tasks\/[^/]+$/.test(new URL(request.url()).pathname)
        )
          writes.push(request.url());
      });
      await activate(page.locator(`.task[data-id="${first.id}"] .task-title`));
      await expect.poll(() => complete(first.id)).toBe(true);
      await expect(page).toHaveURL(originalUrl);
      await expect(page.locator('.planning-detail-panel')).toHaveCount(0);
      const secondRow = page.locator(`.task[data-id="${second.id}"]`);
      const bounds = await secondRow.boundingBox();
      await activate(secondRow, {
        position: { x: bounds.width / 2, y: bounds.height - 8 },
      });
      await expect.poll(() => complete(second.id)).toBe(true);
      await expect(page).toHaveURL(originalUrl);
      await expect(page.locator('.planning-detail-panel')).toHaveCount(0);
      const parentRow = page.locator(`.task[data-id="${parent.id}"]`);
      await parentRow.click({ position: { x: 16, y: 6 } });
      const panel = page.locator('.planning-detail-panel');
      const title = panel.locator(
        `[data-child-id="${child.id}"] .task-page-child-line > a`,
      );
      await expect(title).toBeVisible();
      await activate(title);
      await expect.poll(() => complete(child.id)).toBe(true);
      await expect(panel.locator('[data-task-page]')).toHaveAttribute(
        'data-task-id',
        parent.id,
      );
      expect(writes).toHaveLength(3);
    });
  });
}

test('double click leaves editor fields alone and keeps parent completion confirmation', async ({
  page,
}) => {
  const { capture, complete } = await setup(page);
  const parent = await capture(`Confirm gesture ${randomUUID()}`);
  await capture('Confirm child', parent.id);
  await page.goto(`/tasks/${parent.id}`);
  await expect(page.locator('[data-connection]')).toHaveText('Up to date');
  await page.locator('[data-edit-title]').dblclick();
  const confirmation = page.getByRole('dialog', { name: 'Confirm change' });
  await expect(confirmation).toBeVisible();
  await confirmation.getByRole('button', { name: 'Cancel', exact: true }).click();
  expect(await complete(parent.id)).toBe(false);
  await page.locator('[data-edit-title]').click();
  const title = page.locator('[data-brief-form] [name=title]');
  await expect(title).toBeVisible();
  await title.dblclick();
  await title.fill('Still an editable draft');
  expect(await complete(parent.id)).toBe(false);
  await expect(page.locator('[data-brief-form]')).toBeVisible();
});
