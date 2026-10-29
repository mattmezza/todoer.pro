import { test, expect } from '@playwright/test';
test('the elusive task can be selected for Home and removed when play is disabled', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/settings');
  const form = page.locator('[data-play-preferences]');
  await form.locator('[name=fun_animations]').check();
  await form.locator('[name=fun_animation]').selectOption('chase');
  await expect
    .poll(
      async () =>
        (await (await page.request.get('/api/v1/notification-preferences')).json())
          .fun_animation,
    )
    .toBe('chase');
  await expect(page.locator('canvas.welcome-chase')).toHaveCount(1);
  await page.goto('/today');
  await expect(page.locator('canvas.welcome-chase')).toBeVisible();
  await expect(page.locator('canvas.welcome-chase')).toHaveAttribute(
    'data-complete',
    'false',
  );
  await page.goto('/settings');
  await expect(form.locator('[name=fun_animation]')).toHaveValue('chase');
  await form.locator('[name=fun_animations]').uncheck();
  await expect(page.locator('canvas.welcome-chase')).toHaveCount(0);
  await page.goto('/today');
  await expect(page.locator('canvas.welcome-chase')).toHaveCount(0);
});
