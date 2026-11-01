import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('suggests unused colors and updates a list from the heading', async ({ page }) => {
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
  const space = await post('/api/v1/workspaces', { title: `Colors ${randomUUID()}` });
  const list = await post('/api/v1/lists', {
    workspace_id: space.id,
    title: 'Blue list',
    color: '#4e89f7',
  });
  for (let i = 0; i < 5; i++) {
    const response = await page.request.get(
      `/api/v1/workspaces/${space.id}/list-color-suggestion`,
    );
    expect(response.ok()).toBeTruthy();
    expect((await response.json()).color).not.toBe('#4e89f7');
  }
  await page.goto(`/lists/${list.id}`);
  const headingPicker = page.locator('.heading-list-color');
  await headingPicker.getByLabel('Change list color').click();
  const saved = page.waitForResponse(
    (response) =>
      response.url().endsWith(`/lists/${list.id}/color`) &&
      response.request().method() === 'PUT',
  );
  await headingPicker.getByRole('button', { name: 'Teal', exact: true }).click();
  expect((await saved).status()).toBe(200);
  await expect
    .poll(
      async () =>
        (await (await page.request.get(`/api/v1/lists/${list.id}/color`)).json()).color,
    )
    .toBe('#14b8a6');
  const suggestion = page.waitForResponse((response) =>
    response.url().endsWith(`/workspaces/${space.id}/list-color-suggestion`),
  );
  await page.locator('[data-list-create-open]').first().click();
  const suggested = (await (await suggestion).json()).color;
  const dialog = page.locator('#list-create-dialog');
  await expect(dialog).toBeVisible();
  await expect(dialog.locator('[name="color"]')).toHaveValue(suggested);
  expect(suggested).not.toBe('#14b8a6');
  await dialog.getByLabel('Choose list color').click();
  await dialog.getByRole('button', { name: 'Rose', exact: true }).click();
  await expect(dialog.locator('[name="color"]')).toHaveValue('#f43f5e');
  await expect(dialog.getByLabel('Custom list color')).toHaveCount(1);
});
