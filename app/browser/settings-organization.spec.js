import { test, expect } from '@playwright/test';
test('Account and AI are separate; capture choices persist and preserve drafts after failure', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/settings');
  await expect(page.locator('[data-capture-preferences]')).toHaveCount(0);
  const navigation = page.getByRole('navigation', { name: 'Settings', exact: true });
  await navigation.getByRole('link', { name: 'Account', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Account', exact: true })).toBeVisible();
  await expect(page.locator('[data-capture-preferences]')).toHaveCount(0);
  await page
    .getByRole('navigation', { name: 'Settings', exact: true })
    .getByRole('link', { name: 'AI', exact: true })
    .click();
  const form = page.locator('[data-capture-preferences]');
  await expect(form).toHaveAttribute('aria-busy', 'false');
  const smart = form.locator('[name=smart_enabled]');
  const dates = form.locator('[name=date_enabled]');
  await smart.check();
  await expect(dates).toBeVisible();
  await dates.check();
  await smart.uncheck();
  await expect(dates).toBeHidden();
  await expect(dates).toBeChecked();
  const instructions = form.locator('[name=ai_instructions]');
  const details = form.locator('[data-ai-instructions]');
  if (!(await details.evaluate((el) => el.open)))
    await details.locator('summary').click();
  await instructions.fill('Keep my task titles concise.');
  await page.route('**/api/v1/capture/preferences', (route) =>
    route.request().method() === 'PUT'
      ? route.fulfill({
          status: 503,
          contentType: 'application/json',
          body: JSON.stringify({ error: { message: 'Try again shortly.' } }),
        })
      : route.continue(),
  );
  await form.getByRole('button', { name: 'Save preferences' }).click();
  await expect(form.getByRole('status')).toHaveText('Try again shortly.');
  await expect(instructions).toHaveValue('Keep my task titles concise.');
  await page.unroute('**/api/v1/capture/preferences');
  await form.getByRole('button', { name: 'Save preferences' }).click();
  await expect(form).toHaveAttribute('aria-busy', 'false');
  await page.reload();
  await expect(form).toHaveAttribute('aria-busy', 'false');
  await expect(smart).not.toBeChecked();
  await expect(dates).toBeHidden();
  await expect(dates).toBeChecked();
  await expect(instructions).toHaveValue('Keep my task titles concise.');
  await smart.check();
  await expect(dates).toBeVisible();
  const checkbox = await smart.boundingBox();
  const label = await smart.locator('..').locator('span').boundingBox();
  expect(Math.abs(checkbox.y - label.y)).toBeLessThan(8);
  await page.screenshot({
    path: '.local/rebuild/ai-settings-desktop.png',
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBeTruthy();
  await page.screenshot({
    path: '.local/rebuild/ai-settings-mobile.png',
    fullPage: true,
  });
  await page.goto('/settings/agents');
  await expect(page).toHaveURL(/\/settings\/ai$/);
});
