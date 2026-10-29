import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';

test('sign-out follows the successful redirect without an error', async ({ page }) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.evaluate(() => import('/assets/site.js').then(() => true));
  await page.locator('.account-menu summary').click();
  await page.getByRole('button', { name: 'Sign out', exact: true }).click();
  await expect(page).toHaveURL(/\/signin$/);
  await expect(
    page.getByRole('button', { name: 'Enter development workspace' }),
  ).toBeVisible();
  await page.goto('/settings?tab=general');
  await expect(page).toHaveURL(/\/signin\?next=/);
  expect(new URL(page.url()).searchParams.get('next')).toBe('/settings?tab=general');
  const api = await page.request.get('/api/v1/account/security');
  expect(api.status()).toBe(401);
  expect((await api.json()).error.code).toBe('unauthorized');
});

async function securityPage(page) {
  const template = await readFile('app/templates/account-security.html', 'utf8');
  await page.route('**/settings/security', async (route) => {
    const response = await route.fetch();
    await route.fulfill({
      response,
      body: (await response.text()).replace(
        '<p>Account security is available on the real authentication instance.</p>',
        template.replace('data-auth-policy-panel', 'data-auth-policy-panel hidden'),
      ),
    });
  });
  await page.route('**/api/v1/account/security', (route) =>
    route.fulfill({
      json: {
        email: 'test@example.invalid',
        has_password: true,
        recovery_codes_remaining: 0,
        totp_enabled: false,
        passkeys: [],
      },
    }),
  );
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/settings/security');
}

test('expired security proof prompts for sign-in and preserves return section', async ({
  page,
}) => {
  await page.route('**/api/v1/account/totp/start', (route) =>
    route.fulfill({
      status: 403,
      json: {
        error: {
          code: 'reauthentication_required',
          message: 'Sign in again to change account security',
        },
      },
    }),
  );
  await securityPage(page);
  await page.locator('[data-totp-start]').click();
  const modal = page.getByRole('dialog');
  await expect(modal).toContainText('For your security');
  await modal.getByRole('button', { name: 'Sign in now' }).click();
  await expect(page).toHaveURL(/\/signin\?next=/);
  const next = new URL(page.url()).searchParams.get('next');
  expect(next).toMatch(/^\/settings\/security#security-section-/);
});

test('displayed recovery codes download as a text file', async ({ page }) => {
  const codes = ['first-test-code', 'second-test-code'];
  await page.route('**/api/v1/account/recovery-codes', (route) =>
    route.fulfill({ json: { recovery_codes: codes } }),
  );
  await securityPage(page);
  await page.locator('[data-recovery-generate]').click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Confirm', exact: true })
    .click();
  await expect(page.locator('[data-recovery-codes]')).toContainText(codes[0]);
  const downloadEvent = page.waitForEvent('download');
  await page.locator('[data-download-recovery]').click();
  const download = await downloadEvent;
  expect(download.suggestedFilename()).toBe('todoer-recovery-codes.txt');
  const text = await readFile(await download.path(), 'utf8');
  for (const code of codes) expect(text).toContain(code);
});

test('password modal validates confirmation and releases controls after failure', async ({
  page,
}) => {
  let requests = 0;
  await page.route('**/api/v1/account/password', (route) => {
    requests++;
    return route.fulfill({
      status: 400,
      json: { error: { code: 'account', message: 'Current password did not match' } },
    });
  });
  await securityPage(page);
  await page.locator('[data-open-password]').click();
  const modal = page.locator('[data-password-dialog]');
  await expect(modal).toBeVisible();
  await modal.getByLabel('Current password', { exact: true }).fill('wrong password');
  await modal
    .getByLabel('New password', { exact: true })
    .fill('a sufficiently long new password');
  await modal
    .getByLabel('Confirm new password', { exact: true })
    .fill('another sufficiently long password');
  await modal.getByRole('button', { name: 'Save password', exact: true }).click();
  expect(requests).toBe(0);
  await modal
    .getByLabel('Confirm new password', { exact: true })
    .fill('a sufficiently long new password');
  await modal.getByRole('button', { name: 'Save password', exact: true }).click();
  await expect(page.locator('.toast')).toContainText('Current password did not match');
  await expect(
    modal.getByRole('button', { name: 'Save password', exact: true }),
  ).toBeEnabled();
  await modal.getByRole('button', { name: 'Close password settings' }).click();
  await expect(modal).not.toBeVisible();
  await expect(page.locator('[data-totp-start]')).toBeEnabled();
});

test('notification bell follows the edge of a right dock', async ({ page }) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const bell = page.locator('[data-notification-bell]');
  const before = await bell.boundingBox();
  await page.evaluate(() => {
    const panel = document.createElement('div');
    panel.className = 'planning-detail-panel';
    panel.style.cssText =
      'position:fixed;right:0;top:0;width:320px;height:100vh;animation:none;transform:none';
    document.body.append(panel);
  });
  await expect
    .poll(async () => (await bell.boundingBox()).x)
    .toBeLessThan(before.x - 300);
  const panel = await page.locator('.planning-detail-panel').boundingBox();
  const shifted = await bell.boundingBox();
  expect(shifted.x + shifted.width).toBeLessThan(panel.x);
  await page.locator('.planning-detail-panel').evaluate((el) => el.remove());
  await expect
    .poll(async () => Math.abs((await bell.boundingBox()).x - before.x))
    .toBeLessThan(2);
});
