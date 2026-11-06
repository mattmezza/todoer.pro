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
  const title = page.locator('[name=title]');
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

test('mobile capture and its loader use the same top position as search', async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.evaluate(async () =>
    (await import('/assets/search-omnibar.js')).openSearch(),
  );
  const search = page.locator('.search-omnibar');
  await expect(search).toBeVisible();
  const top = (await search.boundingBox()).y;
  expect(top).toBeLessThan(40);
  await search.getByRole('button', { name: 'Close quick search' }).click();
  let release;
  const delayed = new Promise((resolve) => {
    release = resolve;
  });
  await page.route(/\/capture(?:\?.*)?$/, async (route) => {
    await delayed;
    await route.continue();
  });
  await page.evaluate(() => {
    import('/assets/quick-capture.js').then((module) => module.openCapture());
  });
  const pending = page.getByRole('dialog', { name: 'Loading quick capture' });
  await expect(pending).toBeVisible();
  expect(Math.abs((await pending.boundingBox()).y - top)).toBeLessThanOrEqual(1);
  release();
  const capture = page.locator('.capture-omnibar:has([name=title])');
  await expect(capture).toBeVisible();
  expect(Math.abs((await capture.boundingBox()).y - top)).toBeLessThanOrEqual(1);
  await page.setViewportSize({ width: 390, height: 420 });
  expect(Math.abs((await capture.boundingBox()).y - top)).toBeLessThanOrEqual(1);
  expect(
    await capture.evaluate((el) => el.getBoundingClientRect().bottom <= innerHeight),
  ).toBe(true);
  await page.unrouteAll({ behavior: 'wait' });
});
