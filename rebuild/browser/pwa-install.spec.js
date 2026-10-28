import { test, expect } from '@playwright/test';

test('installation metadata and service worker work outside list pages', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/settings/general');
  await expect(page.locator('link[rel="manifest"]')).toHaveCount(1);
  await expect(page.locator('link[rel="apple-touch-icon"]')).toHaveAttribute(
    'sizes',
    '180x180',
  );
  await expect(page.locator('[data-install-status]')).toContainText('Add to Home Screen');
  await page.waitForFunction(
    async () => !!(await navigator.serviceWorker.getRegistration('/'))?.active,
  );
  const manifest = await (await page.request.get('/assets/manifest.webmanifest')).json();
  expect(manifest.display).toBe('standalone');
  expect(manifest.scope).toBe('/');
  expect(manifest.icons.some((icon) => icon.sizes === '192x192')).toBeTruthy();
  expect(manifest.icons.some((icon) => icon.purpose === 'maskable')).toBeTruthy();
  for (const path of ['/today', '/notifications', '/settings/notifications', '/signin']) {
    await page.goto(path);
    await expect(page.locator('link[rel="manifest"]')).toHaveCount(1);
  }
  await page.goto('/settings/notifications');
  await expect(page.locator('input[name="timezone"]')).toHaveCount(0);
  await expect(
    page.getByRole('link', { name: 'Change in General settings…' }),
  ).toHaveAttribute('href', '/settings/general');
});
