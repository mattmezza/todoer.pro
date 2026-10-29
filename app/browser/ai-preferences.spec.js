import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';

for (const [path, template, enabled] of [
  ['/settings/ai', 'ai-billing-settings.html', false],
  ['/settings/ai/setup', 'ai-setup.html', true],
]) {
  test(`${path} saves only its own AI choices`, async ({ page }) => {
    const policy = {
      user_id: 'synthetic-preferences',
      user_ai_enabled: enabled,
      managed_ai_allowed: true,
      byok_allowed: true,
      funding: 'credits',
      model_id: 'synthetic/test-model',
      key_configured: false,
      key_status: 'absent',
      access_request: null,
      revision: 5,
    };
    const writes = [];
    const content = readFileSync(`app/templates/${template}`, 'utf8');
    await page.route(`**${path}`, (route) =>
      route.fulfill({
        contentType: 'text/html',
        body: `<!doctype html><html><head><meta name="csrf-token" content="synthetic"></head><body>${content}</body></html>`,
      }),
    );
    await page.route('**/api/account/billing', (route) =>
      route.fulfill({ json: { enabled: true, wallet: { available_micro: 1000000 } } }),
    );
    await page.route('**/api/account/ai/**', (route) => {
      const requestPath = new URL(route.request().url()).pathname;
      if (requestPath.endsWith('/models'))
        return route.fulfill({
          json: {
            models: [
              {
                id: policy.model_id,
                name: 'Synthetic',
                enabled: true,
                input_micro_per_million: 1,
                output_micro_per_million: 2,
              },
            ],
          },
        });
      if (requestPath.endsWith('/notices'))
        return route.fulfill({ json: { notices: [] } });
      if (route.request().method() === 'PUT') writes.push(route.request().postDataJSON());
      return route.fulfill({ json: policy });
    });
    await page.goto(path);
    await expect(page.locator('[data-ai-billing]')).toHaveAttribute('aria-busy', 'false');
    if (path === '/settings/ai') {
      await expect(page.getByLabel('Pay for AI with')).toHaveCount(0);
      await page.getByLabel('Allow built-in AI').check();
      await page.getByRole('button', { name: 'Save AI preferences' }).click();
    } else {
      await expect(page.getByLabel('Allow built-in AI')).toHaveCount(0);
      await page.getByRole('button', { name: 'Save AI setup' }).click();
    }
    await expect.poll(() => writes.length).toBe(1);
    expect(writes[0]).toEqual({
      user_ai_enabled: true,
      funding: 'credits',
      model_id: policy.model_id,
      revision: 5,
    });
  });
}
