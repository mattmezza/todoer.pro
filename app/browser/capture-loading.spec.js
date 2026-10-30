import { test, expect } from '@playwright/test';
test('capture preview waits for options without blocking typing or flashing literal interpretation', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  let release;
  const gate = new Promise((resolve) => {
    release = resolve;
  });
  await page.route('**/api/v1/capture/options', async (route) => {
    await gate;
    await route.continue();
  });
  await page.goto('/capture?list=inbox');
  const title = page.locator('input[name=title]');
  await title.fill('Capture while loading p0');
  const preview = page.locator('.capture-result');
  await expect(preview).toHaveAttribute('data-loading-pending', 'text');
  await expect(preview.locator('.skeleton-line')).toBeVisible();
  const priority = page.locator('.capture-badges > .capture-priority');
  await expect(priority).toBeHidden();
  await expect(title).toBeEnabled();
  release();
  await expect(preview).not.toHaveAttribute('data-loading-pending');
  await expect(priority).toHaveText('P0');
  await expect(priority).toBeVisible();
});
