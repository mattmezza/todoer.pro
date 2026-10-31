import { test, expect } from '@playwright/test';

test('Today places space and priority filters above centered date filters', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/today');
  const filters = page.locator('.planning-secondary-filters');
  const dates = page.getByRole('navigation', { name: 'Planning', exact: true });
  await expect(filters).toBeVisible();
  await expect(dates).toBeVisible();
  const filterBox = await filters.boundingBox();
  const dateBox = await dates.boundingBox();
  const mainBox = await page.locator('main').boundingBox();
  expect(dateBox.y).toBeGreaterThan(filterBox.y + filterBox.height - 1);
  expect(
    Math.abs(dateBox.x + dateBox.width / 2 - (mainBox.x + mainBox.width / 2)),
  ).toBeLessThan(3);
});
