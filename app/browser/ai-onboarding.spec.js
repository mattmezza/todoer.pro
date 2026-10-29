import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';

const model = {
  id: 'synthetic/ready-model',
  name: 'Synthetic model',
  enabled: true,
  input_micro_per_million: 1000,
  output_micro_per_million: 2000,
};
async function fixture(
  page,
  overrides = {},
  availableModels = [model],
  availableCredits = 1000000,
) {
  let policy = {
    user_id: 'synthetic-onboarding',
    user_ai_enabled: false,
    managed_ai_allowed: false,
    byok_allowed: true,
    funding: 'none',
    model_id: null,
    key_configured: false,
    key_status: 'absent',
    access_request: null,
    revision: 1,
    billing_blocked: false,
    ...overrides,
  };
  const writes = [];
  for (const [path, template] of [
    ['/settings/ai', 'ai-billing-settings.html'],
    ['/settings/ai/setup', 'ai-setup.html'],
  ]) {
    const content = readFileSync(`app/templates/${template}`, 'utf8');
    await page.route(
      (url) => url.pathname === path,
      (route) =>
        route.fulfill({
          contentType: 'text/html',
          body: `<!doctype html><html><head><meta name="csrf-token" content="synthetic"></head><body>${content}</body></html>`,
        }),
    );
  }
  await page.route('**/api/account/billing', (route) =>
    route.fulfill({
      json: { enabled: true, wallet: { available_micro: availableCredits } },
    }),
  );
  await page.route('**/api/account/ai/**', (route) => {
    const path = new URL(route.request().url()).pathname;
    const method = route.request().method();
    if (path.endsWith('/models'))
      return route.fulfill({ json: { models: availableModels } });
    if (path.endsWith('/notices')) return route.fulfill({ json: { notices: [] } });
    if (method !== 'GET') {
      const payload = route.request().postDataJSON();
      writes.push({ path, payload });
      if (path.endsWith('/key'))
        policy = {
          ...policy,
          funding: 'byok',
          model_id: payload.model_id,
          key_configured: true,
          key_status: 'valid',
          revision: policy.revision + 1,
        };
      if (path.endsWith('/access-request'))
        policy = { ...policy, access_request: 'pending' };
      if (path.endsWith('/settings'))
        policy = { ...policy, ...payload, revision: policy.revision + 1 };
    }
    return route.fulfill({ json: policy });
  });
  return writes;
}
async function ready(page, path) {
  await page.goto(path);
  await expect(page.locator('[data-ai-billing]')).toHaveAttribute('aria-busy', 'false');
}

test('first-use AI preferences guide setup instead of submitting incomplete settings', async ({
  page,
}) => {
  const writes = await fixture(page);
  await ready(page, '/settings/ai');
  await expect(page.getByLabel('Allow built-in AI')).toBeHidden();
  await expect(page.getByRole('link', { name: 'Set up and enable AI' })).toHaveAttribute(
    'href',
    '/settings/ai/setup?enable=1',
  );
  await expect(page.locator('[data-ai-notice]')).toBeHidden();
  expect(writes).toEqual([]);
});

test('guided BYOK setup requires a separate explicit enable action after connecting a key', async ({
  page,
}) => {
  const writes = await fixture(page);
  await ready(page, '/settings/ai/setup?enable=1');
  const enable = page.getByRole('button', { name: 'Enable AI', exact: true });
  await expect(enable).toBeDisabled();
  await expect(page.getByRole('button', { name: /Refresh/ })).toHaveCount(0);
  await page.getByRole('radio', { name: 'My OpenRouter key' }).check();
  await page.getByLabel('2. Choose a model').selectOption(model.id);
  await expect(enable).toBeDisabled();
  await page.getByLabel('OpenRouter API key').fill('synthetic-key-never-real');
  await page.getByRole('button', { name: 'Validate and save key' }).click();
  await expect(enable).toBeEnabled();
  expect(writes).toHaveLength(1);
  expect(writes[0].path).toMatch(/\/key$/);
  await expect(page.getByLabel('OpenRouter API key')).toHaveValue('');
  await expect(page.locator('[data-ai-setup-next]')).toContainText('Enable AI when');
  await enable.click();
  await expect.poll(() => writes.length).toBe(2);
  expect(writes[1].payload).toEqual({
    user_ai_enabled: true,
    funding: 'byok',
    model_id: model.id,
    revision: 2,
  });
  await expect(page).toHaveURL(/\/settings\/ai\/setup$/);
  await expect(page.locator('[data-ai-notice]')).toContainText('AI is enabled');
});

test('managed AI requests explain approval before allowing enable', async ({ page }) => {
  const writes = await fixture(page);
  await ready(page, '/settings/ai/setup?enable=1');
  await page.getByRole('radio', { name: 'Todoer credits', exact: false }).check();
  await page.getByLabel('2. Choose a model').selectOption(model.id);
  await expect(page.locator('[data-ai-setup-next]')).toContainText(
    'Request access above',
  );
  await page.getByRole('button', { name: 'Request access', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Request pending' })).toBeDisabled();
  await page.getByRole('radio', { name: 'Todoer credits', exact: false }).check();
  await expect(
    page.getByRole('button', { name: 'Enable AI', exact: true }),
  ).toBeDisabled();
  expect(writes.map((write) => write.path)).toEqual(['/api/account/ai/access-request']);
});

test('an instance without enabled models explains availability and keeps AI disabled', async ({
  page,
}) => {
  const writes = await fixture(page, {}, []);
  await ready(page, '/settings/ai/setup?enable=1');
  await expect(page.locator('[data-ai-model-availability]')).toContainText(
    'No supported models are available',
  );
  await expect(page.getByLabel('2. Choose a model')).toBeDisabled();
  await expect(
    page.getByRole('button', { name: 'Enable AI', exact: true }),
  ).toBeDisabled();
  expect(writes).toEqual([]);
});

test('users can always turn AI off even when their saved setup is no longer available', async ({
  page,
}) => {
  const writes = await fixture(
    page,
    { user_ai_enabled: true, funding: 'credits', model_id: model.id },
    [],
  );
  await ready(page, '/settings/ai');
  await page.getByLabel('Allow built-in AI').uncheck();
  await page.getByRole('button', { name: 'Save AI preferences' }).click();
  await expect.poll(() => writes.length).toBe(1);
  expect(writes[0].payload).toEqual({
    user_ai_enabled: false,
    funding: 'credits',
    model_id: model.id,
    revision: 1,
  });
});

test('ordinary setup save does not enable AI without the explicit first-use intent', async ({
  page,
}) => {
  const writes = await fixture(page, {
    funding: 'credits',
    model_id: model.id,
    managed_ai_allowed: true,
  });
  await ready(page, '/settings/ai/setup');
  await page.getByRole('button', { name: 'Save AI setup', exact: true }).click();
  await expect.poll(() => writes.length).toBe(1);
  expect(writes[0].payload.user_ai_enabled).toBe(false);
});

test('guided credit setup directs an empty wallet to Billing before enabling AI', async ({
  page,
}) => {
  const writes = await fixture(
    page,
    { funding: 'credits', model_id: model.id, managed_ai_allowed: true },
    [model],
    0,
  );
  await ready(page, '/settings/ai/setup?enable=1');
  await expect(page.locator('[data-ai-setup-next]')).toContainText(
    'Add credits in Billing',
  );
  await expect(
    page.getByRole('button', { name: 'Enable AI', exact: true }),
  ).toBeDisabled();
  await expect(page.getByRole('link', { name: 'Buy or manage credits' })).toHaveAttribute(
    'href',
    '/settings/billing',
  );
  expect(writes).toEqual([]);
});
