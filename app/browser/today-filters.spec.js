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

test('mobile navigation and page actions stay usable', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 700 });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/inbox');
  const nav = page.getByRole('banner', { name: 'Mobile navigation' });
  await expect(nav.locator('a, button')).toHaveText([
    'Menu',
    'Search',
    'Add',
    'Inbox',
    'Today',
  ]);
  const bell = page.locator('[data-notification-bell]');
  const action = page.locator('[data-export-workbench]');
  const bellBox = await bell.boundingBox();
  const actionBox = await action.boundingBox();
  expect(bellBox.x).toBeGreaterThanOrEqual(actionBox.x + actionBox.width - 1);
  await page.evaluate(() => {
    document.body.style.minHeight = '2000px';
  });
  await page.evaluate(() => window.scrollTo(0, 300));
  await expect
    .poll(async () => (await bell.boundingBox()).y)
    .toBeLessThan(bellBox.y - 100);
  await page.goto('/today');
  const dates = page.getByRole('navigation', { name: 'Planning', exact: true });
  const selected = dates.locator('[aria-current="page"]');
  await expect(selected).toHaveText('Today');
  const datesBox = await dates.boundingBox();
  const selectedBox = await selected.boundingBox();
  expect(
    Math.abs(selectedBox.x + selectedBox.width / 2 - (datesBox.x + datesBox.width / 2)),
  ).toBeLessThan(3);
});

test('mobile drawer opens full height and dismisses accessibly', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 700 });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/today');
  const menu = page.getByRole('button', { name: 'Toggle navigation' });
  const drawer = page.locator('#navigation');
  await expect(drawer).toHaveAttribute('inert', '');
  await menu.click();
  await expect(menu).toHaveAttribute('aria-expanded', 'true');
  await expect(drawer).not.toHaveAttribute('inert', '');
  await expect(drawer).toBeVisible();
  const box = await drawer.boundingBox();
  expect(box.height).toBe(700);
  await page.keyboard.press('Escape');
  await expect(menu).toHaveAttribute('aria-expanded', 'false');
  await expect(menu).toBeFocused();
  await menu.click();
  await page
    .getByRole('button', { name: 'Close navigation' })
    .click({ position: { x: 380, y: 200 } });
  await expect(menu).toHaveAttribute('aria-expanded', 'false');
});
