import { test, expect } from '@playwright/test';

test('installation metadata and service worker work outside list pages', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await expect(page.locator('.pwa-install-notice')).toHaveCount(0);
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

test.describe('mobile first-use installation suggestion', () => {
  test.use({
    userAgent:
      'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 Mobile/15E148 Safari/604.1',
    viewport: { width: 390, height: 844 },
    isMobile: true,
    hasTouch: true,
  });
  test('shows once with Home Screen guidance and offers a native prompt when available', async ({
    page,
  }) => {
    await page.goto('/signin');
    await page.getByRole('button', { name: 'Enter development workspace' }).click();
    const notice = page.locator('.pwa-install-notice');
    await expect(notice).toBeVisible();
    await expect(notice).toContainText('Share → Add to Home Screen');
    await page.evaluate(() => {
      const event = new Event('beforeinstallprompt', { cancelable: true });
      event.prompt = async () => {
        window.installInvoked = true;
      };
      event.userChoice = Promise.resolve({ outcome: 'accepted' });
      window.dispatchEvent(event);
    });
    await notice.getByRole('button', { name: 'Install Todoer', exact: true }).click();
    expect(await page.evaluate(() => window.installInvoked)).toBeTruthy();
    await expect(notice).toHaveCount(0);
    await page.reload();
    await expect(notice).toHaveCount(0);
  });
  test('already installed apps are not prompted', async ({ page }) => {
    await page.addInitScript(() =>
      Object.defineProperty(navigator, 'standalone', { value: true }),
    );
    await page.goto('/signin');
    await page.getByRole('button', { name: 'Enter development workspace' }).click();
    await expect(page.locator('.pwa-install-notice')).toHaveCount(0);
  });
});
