import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('appearance saves sidebar visibility and independent swipe choices', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/settings/appearance');
  await page.getByRole('checkbox', { name: 'Show favorites in the sidebar' }).uncheck();
  await page.getByLabel('Swipe left', { exact: true }).selectOption('complete');
  await page.getByLabel('Swipe right', { exact: true }).selectOption('none');
  await page.getByRole('button', { name: 'Save appearance' }).click();
  await expect(page.getByLabel('Swipe left', { exact: true })).toHaveValue('complete');
  await expect(page.getByLabel('Swipe right', { exact: true })).toHaveValue('none');
  await page.goto('/spaces');
  await expect(page.locator('#navigation')).toHaveAttribute(
    'data-swipe-left',
    'complete',
  );
  await expect(page.locator('#navigation')).toHaveAttribute('data-swipe-right', 'none');
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const id = randomUUID();
  const response = await page.request.put(`/api/v1/section-preferences/${id}`, {
    headers: { 'X-CSRF-Token': csrf },
    data: { collapsed: true },
  });
  expect(response.ok()).toBeTruthy();
  const preferences = await (
    await page.request.get('/api/v1/notification-preferences')
  ).json();
  expect(preferences.collapsed_sections).toContain(id);
  expect(preferences.show_favorites).toBe(false);
  expect(preferences.swipe_right).toBe('none');
  await page.reload();
  expect(
    JSON.parse(await page.locator('#navigation').getAttribute('data-collapsed-sections')),
  ).toContain(id);
  await page.request.put(`/api/v1/section-preferences/${id}`, {
    headers: { 'X-CSRF-Token': csrf },
    data: { collapsed: false },
  });
  await page.goto('/settings/appearance');
  await page.getByRole('checkbox', { name: 'Show favorites in the sidebar' }).check();
  await page.getByLabel('Swipe left', { exact: true }).selectOption('archive');
  await page.getByLabel('Swipe right', { exact: true }).selectOption('complete');
  await page.getByRole('button', { name: 'Save appearance' }).click();
});

test('section collapse survives reload and a second browser tab', async ({
  page,
  context,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  async function post(path, data) {
    const response = await page.request.post(`/api/v1${path}`, {
      headers: { 'X-CSRF-Token': csrf, 'Idempotency-Key': randomUUID() },
      data,
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    return response.json();
  }
  const space = await post('/workspaces', { title: `Collapse ${randomUUID()}` });
  const list = await post('/lists', {
    workspace_id: space.id,
    title: 'Remember sections',
  });
  const section = await post(`/lists/${list.id}/sections`, { title: 'Later' });
  await page.goto(`/lists/${list.id}`);
  const disclosure = page.locator(`[data-section-id="${section.id}"] .section-collapse`);
  await expect(disclosure).toHaveAttribute('open', '');
  const saved = page.waitForResponse(
    (response) =>
      response.url().endsWith(`/section-preferences/${section.id}`) &&
      response.request().method() === 'PUT',
  );
  await disclosure.locator('summary').click();
  expect((await saved).ok()).toBeTruthy();
  await page.reload();
  await expect(disclosure).not.toHaveAttribute('open');
  const second = await context.newPage();
  await second.goto(`/lists/${list.id}`);
  await expect(
    second.locator(`[data-section-id="${section.id}"] .section-collapse`),
  ).not.toHaveAttribute('open');
  await second.close();
});
