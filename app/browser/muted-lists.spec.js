import { test, expect } from '@playwright/test';
test('muted lists are searchable, removable, and saved without losing selections', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/settings/notifications');
  const panel = page.locator('.muted-lists-card');
  const search = page.getByRole('searchbox', { name: 'Find a list to mute' });
  await expect(panel.getByText('All your lists can notify you.')).toBeVisible();
  await panel.screenshot({ path: '.local/rebuild/muted-lists-empty.png' });
  await search.fill('Groceries');
  const result = page.locator('.mute-search-results button').first();
  await expect(result).toContainText('Groceries');
  await result.click();
  await expect(page.locator('[data-muted-count]')).toHaveText('1 muted');
  await expect(page.locator('.muted-list-row')).toHaveCount(1);
  await page.getByRole('button', { name: 'Save notification preferences' }).click();
  await expect
    .poll(
      async () =>
        (await (await page.request.get('/api/v1/notification-preferences')).json())
          .list_opt_out.length,
    )
    .toBe(1);
  await page.reload();
  await expect(page.locator('.muted-list-row')).toHaveCount(1);
  await expect(page.locator('.muted-list-row')).toContainText('Groceries');
  await search.fill('zzzz-no-such-list');
  await expect(page.getByText('No matching lists.', { exact: true })).toBeVisible();
  await search.press('Escape');
  await expect(page.locator('.mute-search-results')).toBeHidden();
  await panel.screenshot({ path: '.local/rebuild/muted-lists-selected.png' });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBeTruthy();
  await panel.screenshot({ path: '.local/rebuild/muted-lists-mobile.png' });
  await page.locator('.muted-list-row button').click();
  await expect(page.locator('.muted-list-row')).toHaveCount(0);
  await page.getByRole('button', { name: 'Save notification preferences' }).click();
  await expect(panel.getByText('All your lists can notify you.')).toBeVisible();
  await expect
    .poll(
      async () =>
        (await (await page.request.get('/api/v1/notification-preferences')).json())
          .list_opt_out,
    )
    .toEqual([]);
});
