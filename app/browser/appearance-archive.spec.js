import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test.beforeEach(async ({ page }) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
});

test('appearance previews both fonts and persists a selection across pages', async ({
  page,
}) => {
  await page.goto('/settings/appearance');
  await expect(
    page.getByRole('heading', { name: 'Appearance', exact: true }),
  ).toBeVisible();
  expect(
    await page
      .locator('.appearance-inter')
      .evaluate((el) => getComputedStyle(el).fontFamily),
  ).toContain('Inter');
  expect(
    await page
      .locator('.appearance-geist')
      .evaluate((el) => getComputedStyle(el).fontFamily),
  ).toContain('Geist');
  await page.getByRole('radio', { name: /Inter/ }).check();
  await page.getByRole('button', { name: 'Save appearance' }).click();
  await expect(page.getByRole('radio', { name: /Inter/ })).toBeChecked();
  await page.goto('/spaces');
  await expect(page.locator('#navigation')).toHaveAttribute('data-font', 'inter');
  expect(
    await page.locator('body').evaluate((el) => getComputedStyle(el).fontFamily),
  ).toContain('Inter');
  await page.goto('/settings/appearance');
  await page.getByRole('radio', { name: /Geist/ }).check();
  await page.getByRole('button', { name: 'Save appearance' }).click();
  await expect(page.locator('#navigation')).toHaveAttribute('data-font', 'geist');
});

test('archived lists are separate, show provenance, and can be restored', async ({
  page,
}) => {
  await page.goto('/spaces');
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const headers = { 'X-CSRF-Token': csrf, 'Idempotency-Key': randomUUID() };
  const response = await page.request.post('/api/v1/workspaces', {
    headers,
    data: { title: `Archive ${randomUUID().slice(0, 8)}`, description: '' },
  });
  expect(response.ok()).toBeTruthy();
  const space = await response.json();
  const listResponse = await page.request.post('/api/v1/lists', {
    headers: { ...headers, 'Idempotency-Key': randomUUID() },
    data: {
      workspace_id: space.id,
      title: 'Archived example',
      description: 'Keep these notes',
      restricted: false,
    },
  });
  expect(listResponse.ok()).toBeTruthy();
  const list = await listResponse.json();
  const captured = await page.request.post('/api/v1/capture', {
    headers: { ...headers, 'Idempotency-Key': randomUUID() },
    data: { list_id: list.id, tasks: [{ title: 'Retained task', description: '' }] },
  });
  expect(captured.ok()).toBeTruthy();
  const [task] = await captured.json();
  const archived = await page.request.patch(`/api/v1/lists/${list.id}`, {
    headers,
    data: { revision: list.revision, archived: true },
  });
  expect(archived.ok()).toBeTruthy();
  await page.goto(`/space/${space.id}`);
  await expect(page.locator('[data-archived-lists]')).toBeHidden();
  await page.getByRole('checkbox', { name: 'Show archived lists' }).check();
  await expect(page.locator('[data-archived-lists]')).toContainText('Archived by you on');
  await page.locator('[data-archived-lists] a').click();
  await expect(page.getByText('This list is archived and read-only.')).toBeVisible();
  await expect(page.locator('body')).toHaveAttribute('data-can-write', 'false');
  await expect(page.locator('[data-list-edit-open]').first()).toBeDisabled();
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
  const banner = page.getByRole('region', { name: 'Archived list', exact: true });
  const bounds = await banner.boundingBox();
  expect(bounds.width).toBeGreaterThan(600);
  expect(bounds.height).toBeLessThan(200);
  expect(bounds.y).toBeLessThan(100);
  await page.goto(`/tasks/${task.id}`);
  await expect(banner).toBeVisible();
  await expect(page.locator('[data-task-page]')).toHaveAttribute(
    'data-can-write',
    'false',
  );
  await page.getByRole('button', { name: 'Restore list…', exact: true }).click();
  const confirmation = page.getByRole('dialog', { name: 'Restore this list?' });
  await expect(confirmation).toContainText('search results');
  await expect(confirmation).toContainText(
    'Existing members and direct shares keep their access',
  );
  await confirmation.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(banner).toBeVisible();
  await page.getByRole('button', { name: 'Restore list…', exact: true }).click();
  await confirmation.getByRole('button', { name: 'Restore list', exact: true }).click();
  await expect(page.locator('[data-task-page]')).toHaveAttribute(
    'data-can-write',
    'true',
  );
  await page.goto(`/lists/${list.id}`);
  await expect(page.locator('body')).toHaveAttribute('data-can-write', 'true');
  await expect(page.locator('.list-archive-banner')).toHaveCount(0);
  await page.goto(`/space/${space.id}`);
  await expect(page.locator('.space-list-grid').first()).toContainText(
    'Archived example',
  );
});
