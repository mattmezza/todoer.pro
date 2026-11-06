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

for (const width of [390, 1280]) {
  test(`optional list emojis persist, follow color changes and can be removed at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 844 });
    await page.goto('/signin');
    await page.getByRole('button', { name: 'Enter development workspace' }).click();
    const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
    const post = async (path, data) => {
      const response = await page.request.post(`/api/v1${path}`, {
        headers: { 'X-CSRF-Token': csrf },
        data,
      });
      expect(response.ok(), await response.text()).toBeTruthy();
      return response.json();
    };
    const space = await post('/workspaces', { title: `Emoji ${randomUUID()}` });
    const list = await post('/lists', {
      workspace_id: space.id,
      title: 'Emoji list',
      color: '#4e89f7',
      emoji: '👩🏽‍💻',
    });
    await page.request.put('/api/v1/favorites', {
      headers: { 'X-CSRF-Token': csrf },
      data: { kind: 'list', id: list.id, favorite: true },
    });
    const appearance = async () =>
      (await page.request.get(`/api/v1/lists/${list.id}/color`)).json();
    await page.goto(`/lists/${list.id}`);
    const picker = page.locator('.heading-list-color');
    const preview = picker.locator('.list-color-preview .list-emoji');
    await expect(preview).toHaveText('👩🏽‍💻');
    await expect(
      page
        .locator(`#navigation .list-navigation[href="/lists/${list.id}"] .list-emoji`)
        .first(),
    ).toHaveText('👩🏽‍💻');
    await picker.getByLabel('Change list color').click();
    await expect(picker.getByLabel('List emoji', { exact: true })).toBeHidden();
    await picker.locator('.list-emoji-field > summary').click();
    await picker.getByRole('button', { name: 'Shopping', exact: true }).click();
    await expect.poll(async () => (await appearance()).emoji).toBe('🛒');
    await picker.getByLabel('Change list color').click();
    await picker.getByRole('button', { name: 'Teal', exact: true }).click();
    await expect.poll(appearance).toEqual({ color: '#14b8a6', emoji: '🛒' });
    await page.reload();
    await expect(preview).toHaveText('🛒');
    await picker.getByLabel('Change list color').click();
    await expect(picker.getByLabel('List emoji', { exact: true })).toBeHidden();
    await picker.locator('.list-emoji-field > summary').click();
    await picker.getByLabel('List emoji', { exact: true }).fill('👨‍👩‍👧‍👦');
    await picker.getByLabel('List emoji', { exact: true }).press('Tab');
    await expect.poll(async () => (await appearance()).emoji).toBe('👨‍👩‍👧‍👦');
    await picker.getByRole('button', { name: 'No emoji', exact: true }).click();
    await expect.poll(appearance).toEqual({ color: '#14b8a6', emoji: '' });
    await expect(preview).toBeEmpty();
    if (width === 390)
      await page.getByRole('button', { name: 'Toggle navigation' }).click();
    await page.locator('[data-list-create-open]').first().click();
    const dialog = page.locator('#list-create-dialog');
    await dialog.getByLabel('List name', { exact: true }).fill('Emoji created in picker');
    await dialog.getByLabel('Choose list color').click();
    await dialog.locator('.list-emoji-field > summary').click();
    await dialog.getByRole('button', { name: 'Travel', exact: true }).click();
    await dialog.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page.locator('.heading-list-color .list-emoji')).toHaveText('✈️');
    await page.goto(`/space/${space.id}`);
    await expect(
      page
        .locator('.space-list-card')
        .filter({ hasText: 'Emoji created in picker' })
        .locator('.list-emoji'),
    ).toHaveText('✈️');
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    ).toBe(true);
  });
}
