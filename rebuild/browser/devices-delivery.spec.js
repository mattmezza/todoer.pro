import { test, expect } from '@playwright/test';
test('Devices and Delivery stay clear on desktop and mobile', async ({ page }) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/settings/devices');
  await expect(page.getByRole('heading', { name: 'This browser', exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Registered devices' })).toBeVisible();
  await expect(page.getByText('No devices yet', { exact: true })).toBeVisible();
  await expect(page.locator('[data-push-enable]')).toHaveCount(0);
  await page.screenshot({ path: '.local/rebuild/devices-desktop.png', fullPage: true });
  await page.goto('/settings/jobs?limit=5');
  await expect(page.getByRole('heading', { name: 'Delivery', exact: true })).toBeVisible();
  await expect(page.locator('.delivery-entry')).toHaveCount(5);
  await expect(page.locator('.delivery-details').first()).not.toHaveAttribute('open');
  const more = page.getByRole('link', { name: 'More deliveries' });
  await expect(more).toHaveAttribute('href', /limit=5/);
  await more.click();
  await expect(page.getByRole('link', { name: 'Back to first page' })).toBeVisible();
  await page.getByRole('combobox', { name: 'Show', exact: true }).selectOption('dead');
  await page.getByRole('button', { name: 'Apply filter' }).click();
  await expect(page.locator('.delivery-entry')).toHaveCount(1);
  await expect(page.locator('.delivery-badge')).toHaveText('Stopped');
  await expect(page.locator('.delivery-reason')).toContainText('retry limit');
  await page.getByText('Delivery details', { exact: true }).click();
  await expect(page.getByText('Attempts', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Refresh', exact: true }).click();
  await expect(page.getByRole('combobox', { name: 'Show', exact: true })).toHaveValue('dead');
  await page.screenshot({ path: '.local/rebuild/delivery-desktop.png', fullPage: true });
  await page.getByRole('combobox', { name: 'Show', exact: true }).selectOption('leased');
  await page.getByRole('button', { name: 'Apply filter' }).click();
  await expect(page.getByRole('heading', { name: 'All clear here' })).toBeVisible();
  await expect(page.getByRole('navigation', { name: 'Delivery pages' })).toHaveCount(0);
  await page.setViewportSize({ width: 390, height: 844 });
  for (const path of ['/settings/devices', '/settings/jobs?status=dead']) {
    await page.goto(path);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.screenshot({ path: `.local/rebuild/${path.includes('devices') ? 'devices' : 'delivery'}-mobile.png`, fullPage: true });
  }
});
