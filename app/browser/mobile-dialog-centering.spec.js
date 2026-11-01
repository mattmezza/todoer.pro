import { test, expect } from '@playwright/test';

test('mobile dialogs center and scroll within the viewport', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/today');
  await page.evaluate(() => document.querySelector('#list-create-dialog').showModal());
  const dialog = page.locator('#list-create-dialog');
  await expect(dialog).toBeVisible();
  const bounds = await dialog.boundingBox();
  expect(Math.abs(bounds.y + bounds.height / 2 - 422)).toBeLessThan(3);
  expect(bounds.height).toBeLessThanOrEqual(812);
  await page.evaluate(() => document.querySelector('#list-create-dialog').close());
  await page.evaluate(async () => {
    const { confirmAction } = await import('/assets/confirm.js');
    void confirmAction('Delete this test item?');
  });
  const confirmation = page.locator('.confirmation-dialog');
  await expect(confirmation).toBeVisible();
  const confirmationBox = await confirmation.boundingBox();
  expect(Math.abs(confirmationBox.y + confirmationBox.height / 2 - 422)).toBeLessThan(3);
});
