import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('undocked space settings fit a phone and keep their controls usable', async ({
  page,
}) => {
  await page.setViewportSize({ width: 375, height: 812 });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
  const result = await page.request.post('/api/v1/workspaces', {
    headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
    data: { title: `Mobile settings ${randomUUID()}` },
  });
  expect(result.ok()).toBeTruthy();
  const space = await result.json();
  await page.goto(`/space/${space.id}`);
  await page.locator('.space-overview-heading .entity-menu summary').click();
  await page.locator('.space-overview-heading [data-workspace-settings-open]').click();
  const panel = page.getByRole('dialog', { name: 'Space settings' });
  await expect(panel).toBeVisible();
  await expect(
    panel.getByRole('heading', { name: 'Members', exact: true }),
  ).toBeVisible();
  await expect(panel.getByLabel('Add a member')).toBeVisible();
  await expect(
    panel.getByRole('link', { name: 'Invite by email…', exact: true }),
  ).toBeVisible();
  await expect(panel.getByRole('heading', { name: 'Public access' })).toBeVisible();
  const sizes = await panel.evaluate((element) => ({
    scroll: element.scrollWidth,
    width: element.clientWidth,
  }));
  expect(sizes.scroll).toBeLessThanOrEqual(sizes.width + 1);
  const form = panel.locator('.member-add');
  const box = await form.boundingBox(),
    input = await panel.getByLabel('Add a member').boundingBox();
  expect(input.x).toBeGreaterThan(box.x);
  expect(input.x + input.width).toBeLessThanOrEqual(box.x + box.width);
});
