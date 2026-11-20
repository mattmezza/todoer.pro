import { expect, test } from '@playwright/test';

test('appearance follows the device and saves an explicit theme across navigation', async ({
  page,
}) => {
  await page.emulateMedia({ colorScheme: 'light' });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/settings/appearance');
  const theme = page.getByLabel('Color scheme');
  const save = page.getByRole('button', { name: 'Save appearance' });
  const body = page.locator('body');

  await expect(theme).toHaveValue('system');
  await expect(body).toHaveCSS('background-color', 'rgb(255, 255, 255)');
  await page.emulateMedia({ colorScheme: 'dark' });
  await expect(body).toHaveCSS('background-color', 'rgb(0, 0, 0)');

  await theme.selectOption('light');
  await save.click();
  await expect(body).toHaveCSS('background-color', 'rgb(255, 255, 255)');
  await page.reload();
  await expect(theme).toHaveValue('light');

  await page.emulateMedia({ colorScheme: 'light' });
  await theme.selectOption('dark');
  await save.click();
  await expect(body).toHaveCSS('background-color', 'rgb(0, 0, 0)');
  await page.goto('/lists/inbox');
  await expect(body).toHaveCSS('background-color', 'rgb(0, 0, 0)');

  await page.goto('/settings/appearance');
  await theme.selectOption('system');
  await save.click();
  await expect(body).toHaveCSS('background-color', 'rgb(255, 255, 255)');
});
