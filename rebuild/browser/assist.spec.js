import { test, expect } from '@playwright/test';
for (const width of [360, 1440]) {
  test(`retired Assist URLs lead to AI settings ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto('/signin');
    await page.getByRole('button', { name: 'Enter development workspace' }).click();
    for (const path of ['/assist', '/settings/assist']) {
      await page.goto(path);
      await expect(page).toHaveURL(/\/settings\/ai$/);
      await expect(page.getByRole('heading', { name: 'AI', exact: true })).toBeVisible();
      await expect(page.locator('[data-capture-preferences]')).toHaveAttribute('aria-busy', 'false');
      await expect(page.locator('a[href="/settings/assist"]')).toHaveCount(0);
    }
    await page.goto('/settings/security');
    await expect(page.getByRole('heading', { name: 'Security', exact: true })).toBeVisible();
    await expect(page).toHaveTitle('Security · Todoer settings');
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  });
}
