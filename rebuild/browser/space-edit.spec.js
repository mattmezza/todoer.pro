import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('space name and description share an inline editor and retain drafts on errors', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/spaces');
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const response = await page.request.post('/api/v1/workspaces', {
    headers: { 'X-CSRF-Token': csrf, 'Idempotency-Key': randomUUID() },
    data: {
      title: `Edit ${randomUUID().slice(0, 8)}`,
      description: 'Original **description**',
    },
  });
  expect(response.ok()).toBeTruthy();
  const workspace = await response.json();
  await page.goto(`/space/${workspace.id}`);
  const form = page.locator('[data-space-edit-form]');
  await page.locator('[data-space-edit="description"]').click();
  await expect(form.locator('[name="description"]')).toBeFocused();
  await expect(form.locator('[name="title"]')).toBeVisible();
  await form.locator('[name="title"]').fill('Edited space');
  await form.locator('[name="description"]').fill('Draft description');
  await page.route('**/api/v1/workspaces/*', async (route) => {
    await route.fulfill({ status: 409, json: { error: { message: 'Stale revision' } } });
  });
  await form.locator('[name="description"]').press('Control+Enter');
  await expect(form.getByRole('alert')).toContainText('Stale revision');
  await expect(form.locator('[name="description"]')).toHaveValue('Draft description');
  await form.locator('[name="description"]').press('Escape');
  await expect(form).toBeHidden();
  await page.unroute('**/api/v1/workspaces/*');
  await page.locator('[data-space-edit="title"]').click();
  await form.locator('[name="title"]').press('Control+Enter');
  await expect(page.locator('[data-space-edit="title"]')).toHaveText('Edited space');
  await expect(page.locator('[data-space-edit="description"]')).toHaveText(
    'Draft description',
  );
});
