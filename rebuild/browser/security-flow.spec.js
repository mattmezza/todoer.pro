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
  const template = await readFile('rebuild/templates/account-security.html', 'utf8');
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
