import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';

test('refresh announces progress, prevents duplicate requests and confirms with a toast', async ({
  page,
}) => {
  const content = readFileSync('app/templates/ai-admin-finance.html', 'utf8');
  await page.route('**/admin/finance', (route) =>
    route.fulfill({
      contentType: 'text/html',
      body: `<!doctype html><html><head><meta name="csrf-token" content="synthetic"></head><body>${content}</body></html>`,
    }),
  );
  let count = 0;
  let release;
  await page.route('**/api/admin/ai/finance*', async (route) => {
    count++;
    if (count === 2)
      await new Promise((resolve) => {
        release = resolve;
      });
    await route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify({ models: [], usage_revenue_micro: 0, contribution_micro: 0 }),
    });
  });
  await page.goto('/admin/finance');
  await expect(page.locator('[data-ai-billing]')).toHaveAttribute('aria-busy', 'false');
  const button = page.locator('[data-ai-reload]');
  await button.click();
  await expect(button).toHaveText('Refreshing…');
  await expect(button).toHaveAttribute('aria-disabled', 'true');
  await expect.poll(() => count).toBe(2);
  await button.dispatchEvent('click');
  expect(count).toBe(2);
  release();
  await expect(button).toHaveText('Refresh this view');
  await expect(button).not.toHaveAttribute('aria-disabled');
  await expect(page.locator('.toast')).toContainText('Up to date');
});

test('failed refresh preserves content and offers visible error feedback', async ({
  page,
}) => {
  const content = readFileSync('app/templates/ai-admin-finance.html', 'utf8');
  await page.route('**/admin/finance', (route) =>
    route.fulfill({
      contentType: 'text/html',
      body: `<!doctype html><html><body>${content}</body></html>`,
    }),
  );
  let count = 0;
  await page.route('**/api/admin/ai/finance*', (route) =>
    route.fulfill({
      status: ++count === 1 ? 200 : 503,
      contentType: 'application/json',
      body: JSON.stringify(
        count === 1
          ? { models: [], topups_micro: 12000000 }
          : { error: 'Temporarily unavailable' },
      ),
    }),
  );
  await page.goto('/admin/finance');
  await expect(page.locator('[data-ai-billing]')).toHaveAttribute('aria-busy', 'false');
  await page.locator('[data-ai-reload]').click();
  await expect(page.locator('.toast')).toHaveText('Temporarily unavailable');
  await expect(page.locator('[data-admin-finance]')).toContainText('12.00');
  await expect(page.locator('[data-ai-reload]')).toHaveText('Refresh this view');
});

test('a form save shows progress, restores the action and confirms success', async ({
  page,
}) => {
  const content = readFileSync('app/templates/ai-billing-settings.html', 'utf8');
  await page.route('**/settings/ai', (route) =>
    route.fulfill({
      contentType: 'text/html',
      body: `<!doctype html><html><body>${content}</body></html>`,
    }),
  );
  let release;
  let saving = false;
  await page.route('**/api/account/ai/settings', async (route) => {
    if (route.request().method() === 'PUT') {
      saving = true;
      await new Promise((resolve) => {
        release = resolve;
      });
    }
    await route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify({
        revision: 1,
        user_ai_enabled: false,
        funding: 'credits',
        managed_ai_allowed: true,
        model_id: 'synthetic/test',
      }),
    });
  });
  await page.route('**/api/account/ai/models', (route) =>
    route.fulfill({
      json: { models: [{ id: 'synthetic/test', name: 'Synthetic', enabled: true }] },
    }),
  );
  await page.route('**/api/account/billing', (route) =>
    route.fulfill({ json: { enabled: true, wallet: { available_micro: 10000000 } } }),
  );
  await page.goto('/settings/ai');
  await expect(page.locator('[data-ai-billing]')).toHaveAttribute('aria-busy', 'false');
  const button = page.locator('[data-ai-policy] button[type=submit]');
  await button.click();
  await expect(button).toHaveText('Saving…');
  await expect(button).toHaveAttribute('aria-disabled', 'true');
  await expect.poll(() => saving).toBe(true);
  release();
  await expect(button).toHaveText('Save AI preferences');
  await expect(page.locator('.toast')).toHaveText('AI preferences saved.');
});
