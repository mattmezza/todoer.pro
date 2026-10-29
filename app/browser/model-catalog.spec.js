import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';

test('model catalog edits one model and adds/removes disabled entries without erasing history', async ({
  page,
}) => {
  const template = readFileSync('app/templates/ai-admin-models.html', 'utf8');
  await page.route('**/admin/models', (route) =>
    route.fulfill({
      contentType: 'text/html',
      body: `<!doctype html><html><head><meta name="csrf-token" content="synthetic"><meta name="viewport" content="width=device-width,initial-scale=1"></head><body><main>${template}</main></body></html>`,
    }),
  );
  let catalog = [
    {
      id: 'vendor/first-model',
      name: 'First model',
      enabled: false,
      classify: true,
      draft: true,
      input_micro_per_million: 1000000,
      output_micro_per_million: 2000000,
      cached_micro_per_million: 0,
      margin_bps: -1,
      daily_limit_micro: 1000000,
      version: 1,
    },
  ];
  const writes = [];
  await page.route('**/api/admin/ai/provider', (route) =>
    route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify({
        enabled: true,
        margin_bps: 2000,
        usd_eur_ppm: 900000,
        daily_limit_micro: 5000000,
        version: 1,
      }),
    }),
  );
  await page.route('**/api/admin/ai/models', (route) => {
    const method = route.request().method();
    if (method !== 'GET') {
      const body = route.request().postDataJSON();
      writes.push({ method, body });
      if (method === 'POST')
        catalog.push({
          ...catalog[0],
          id: body.id,
          name: body.name,
          enabled: false,
          classify: false,
          draft: false,
          version: 1,
        });
      if (method === 'DELETE') catalog = catalog.filter((model) => model.id !== body.id);
      if (method === 'PUT')
        catalog = catalog.map((model) =>
          model.id === body.id ? { ...body, version: model.version + 1 } : model,
        );
    }
    return route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify(method === 'GET' ? { models: catalog } : { saved: true }),
    });
  });
  await page.goto('/admin/models');
  await expect(page.locator('.ai-admin-model')).toHaveCount(0);
  await page.getByRole('button', { name: 'Edit First model', exact: true }).click();
  await expect(page.locator('.ai-admin-model')).toHaveCount(1);
  await page.getByRole('button', { name: 'Add model', exact: true }).click();
  const add = page.locator('[data-admin-model-create]');
  await add.getByLabel('OpenRouter model ID').fill('vendor/second-model');
  await add.getByLabel('Display name').fill('Second model');
  await add.getByRole('button', { name: 'Add model', exact: true }).click();
  await expect(page.locator('.ai-admin-model h3')).toHaveText('Second model');
  await expect(page.locator('.ai-admin-model input[name=enabled]')).not.toBeChecked();
  expect(writes[0]).toEqual({
    method: 'POST',
    body: { id: 'vendor/second-model', name: 'Second model' },
  });
  page.once('dialog', (dialog) => dialog.accept());
  await page.getByRole('button', { name: 'Remove model', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'Edit Second model', exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByRole('button', { name: 'Edit First model', exact: true }),
  ).toBeVisible();
  expect(writes[1]).toEqual({
    method: 'DELETE',
    body: { id: 'vendor/second-model', version: 1 },
  });
});
