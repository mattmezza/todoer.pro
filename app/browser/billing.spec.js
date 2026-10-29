import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';

// Isolate presentation states from real providers, payments and account data.
// Domain and HTTP tests separately cover authorization and money invariants.
async function shell(page, path, template) {
  const content = readFileSync(`app/templates/${template}`, 'utf8');
  await page.route(`**${path}`, (route) =>
    route.fulfill({
      contentType: 'text/html',
      body: `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="csrf-token" content="synthetic-csrf"><meta name="account-id" content="synthetic-billing-user"></head><body><main>${content}</main></body></html>`,
    }),
  );
}
function json(route, data, status = 200) {
  return route.fulfill({
    status,
    contentType: 'application/json',
    body: JSON.stringify(data),
  });
}
const model = {
  id: 'synthetic/test-model',
  name: 'Synthetic supported model',
  enabled: true,
  input_micro_per_million: 1000000,
  output_micro_per_million: 2000000,
  cached_micro_per_million: 500000,
  version: 1,
};

test('BYOK setup clears the key and preserves the personal AI switch', async ({
  page,
}) => {
  await shell(page, '/settings/ai/setup', 'ai-setup.html');
  let policy = {
    user_id: 'synthetic-billing-user',
    user_ai_enabled: false,
    managed_ai_allowed: false,
    byok_allowed: true,
    funding: 'none',
    model_id: model.id,
    key_configured: false,
    key_status: 'absent',
    access_request: null,
    revision: 1,
  };
  const writes = [];
  await page.route('**/api/account/ai/**', async (route) => {
    const path = new URL(route.request().url()).pathname;
    const method = route.request().method();
    if (method !== 'GET') writes.push({ path, body: route.request().postDataJSON() });
    if (path.endsWith('/models')) return json(route, { models: [model] });
    if (path.endsWith('/notices')) return json(route, { notices: [] });
    if (path.endsWith('/key')) {
      policy = {
        ...policy,
        funding: 'byok',
        key_configured: true,
        key_status: 'valid',
        revision: 2,
      };
      return json(route, policy);
    }
    return json(route, policy);
  });
  await page.goto('/settings/ai/setup');
  await expect(page.locator('[data-ai-billing]')).toHaveAttribute('aria-busy', 'false');
  await page.getByRole('radio', { name: 'My OpenRouter key' }).check();
  await page.getByLabel('OpenRouter API key').fill('synthetic-key-never-real');
  await page.locator('[data-ai-policy] select').selectOption(model.id);
  await page.getByRole('button', { name: 'Validate and save key' }).click();
  await expect(page.getByLabel('OpenRouter API key')).toHaveValue('');
  await expect(page.getByLabel('Allow built-in AI')).toHaveCount(0);
  await expect(page.getByRole('radio', { name: 'My OpenRouter key' })).toBeChecked();
  await expect(page.locator('[data-ai-notice]')).toContainText(
    'Your AI feature switch was preserved',
  );
  expect(writes).toHaveLength(1);
  expect(writes[0].body.key).toBe('synthetic-key-never-real');
  expect(await page.locator('body').textContent()).not.toContain(
    'synthetic-key-never-real',
  );
});

test('an uncertain top-up retains its payment idempotency key across reload', async ({
  page,
}) => {
  await shell(page, '/settings/billing', 'billing-settings.html');
  const keys = [];
  const state = {
    enabled: true,
    wallet: {
      posted_micro: 10000000,
      held_micro: 1000000,
      available_micro: 9000000,
      paid_micro: 7000000,
      free_micro: 3000000,
      currency: 'EUR',
    },
    purchases: [],
    auto_topup: {
      enabled: false,
      amount_minor: 1000,
      threshold_minor: 200,
      monthly_cap_minor: 5000,
      cooldown_seconds: 3600,
      paused_reason: null,
      payment_method_configured: false,
    },
    currencies: [{ code: 'EUR', eur_rate_micros: 1000000 }],
  };
  await page.route('**/api/account/billing**', async (route) => {
    if (route.request().method() === 'POST') {
      keys.push(route.request().postDataJSON().idempotency_key);
      if (keys.length === 1) return route.abort('failed');
      return json(route, { purchase_id: 'synthetic-purchase', status: 'pending' });
    }
    return json(route, state);
  });
  await page.goto('/settings/billing');
  await expect(page.locator('[data-ai-billing]')).toHaveAttribute('aria-busy', 'false');
  await expect(page.getByLabel('Balance threshold (€)')).toHaveValue('2.00');
  await page.getByRole('button', { name: 'Continue to secure checkout' }).click();
  await expect(page.locator('[data-ai-notice]')).toContainText('could not be confirmed');
  await page.reload();
  await expect(page.locator('[data-ai-billing]')).toHaveAttribute('aria-busy', 'false');
  await page.getByRole('button', { name: 'Continue to secure checkout' }).click();
  await expect(page.locator('[data-ai-notice]')).toContainText(
    'only after payment confirmation',
  );
  expect(keys).toHaveLength(2);
  expect(keys[0]).toBe(keys[1]);
  await expect(page.locator('[data-wallet]')).toContainText('€9.00');
  await expect(page.locator('[data-wallet]')).toContainText('Available to spend');
  await expect(page.locator('[data-wallet-pending]')).toContainText('€1.00');
  await page.getByText('Balance details', { exact: true }).click();
  await expect(page.locator('[data-wallet-details]')).toContainText('Purchased credits');
  await expect(page.locator('[data-wallet-details]')).toContainText('€7.00');
  await expect(page.locator('[data-wallet-details]')).toContainText('Free credits');
  await expect(page.locator('[data-wallet-details]')).toContainText('€3.00');
});

test('foreign-currency payment previews deductions and submits the reviewed net credit', async ({
  page,
}) => {
  await shell(page, '/settings/billing', 'billing-settings.html');
  let submitted;
  const state = {
    enabled: true,
    wallet: {
      posted_micro: 0,
      held_micro: 0,
      available_micro: 0,
      paid_micro: 0,
      free_micro: 0,
    },
    purchases: [],
    auto_topup: {
      enabled: false,
      amount_minor: 1000,
      threshold_minor: 200,
      monthly_cap_minor: 5000,
      cooldown_seconds: 3600,
      paused_reason: '',
      payment_method_configured: false,
    },
    currencies: [
      {
        code: 'EUR',
        eur_rate_micros: 1000000,
        payment_fee_bps: 200,
        payment_fee_fixed_micro: 200000,
      },
      {
        code: 'USD',
        eur_rate_micros: 1250000,
        payment_fee_bps: 200,
        payment_fee_fixed_micro: 200000,
        forex_fee_bps: 100,
      },
    ],
  };
  await page.route('**/api/account/billing**', async (route) => {
    if (route.request().method() === 'POST') {
      submitted = route.request().postDataJSON();
      return json(
        route,
        {
          error:
            'Conversion changed. Refresh billing and review the credit amount before paying.',
        },
        409,
      );
    }
    return json(route, state);
  });
  await page.goto('/settings/billing');
  await expect(page.locator('[data-ai-billing]')).toHaveAttribute('aria-busy', 'false');
  await page.getByLabel('Pay in').selectOption('USD');
  await expect(page.locator('[data-topup-preset="5"]')).toBeDisabled();
  await page.locator('[data-topup-preset="10"]').click();
  await expect(page.getByLabel('Payment amount', { exact: true })).toHaveValue('10.00');
  await expect(page.locator('[data-topup-conversion]')).toContainText('€7.56');
  await expect(page.locator('[data-topup-conversion]')).toContainText('€0.36');
  await expect(page.locator('[data-topup-conversion]')).toContainText('€0.08');
  await page.getByRole('button', { name: 'Continue to secure checkout' }).click();
  await expect(page.locator('[data-ai-notice]')).toContainText('Conversion changed');
  expect(submitted.amount_minor).toBe(1000);
  expect(submitted.currency).toBe('USD');
  expect(submitted.quoted_credit_micro).toBe(7560000);
});

for (const width of [360, 1440]) {
  test(`usage renders untrusted metadata as text and separates BYOK charges at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 900 });
    await shell(page, '/settings/ai/usage', 'ai-usage.html');
    await page.route('**/api/account/ai/usage?**', (route) =>
      json(route, {
        operations: [
          {
            id: 'synthetic-operation',
            kind: 'draft',
            funding: 'byok',
            model_id: '<img src=x onerror="window.injected=true">',
            provider: 'Synthetic provider',
            status: 'settled',
            input_tokens: 30,
            output_tokens: 20,
            cached_tokens: 0,
            reasoning_tokens: null,
            customer_micro: 0,
            byok_cost_micro_usd: 12345,
            provider_cost_micro: 999999999,
            created_at: 1793275200,
          },
        ],
        next_before: null,
      }),
    );
    await page.goto('/settings/ai/usage');
    await expect(page.locator('[data-ai-billing]')).toHaveAttribute('aria-busy', 'false');
    await expect(page.locator('[data-usage-rows]')).toContainText('Todoer €0.00');
    await expect(page.locator('[data-usage-rows]')).toContainText('OpenRouter $0.012345');
    await expect(page.locator('[data-usage-rows] img')).toHaveCount(0);
    expect(await page.evaluate(() => window.injected)).toBeUndefined();
    expect(await page.locator('body').textContent()).not.toContain('999999999');
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    ).toBe(true);
  });
}

test('admin forms submit revision-bound access and prospective pricing changes', async ({
  page,
}) => {
  for (const screen of ['access', 'models', 'finance', 'operations'])
    await shell(page, `/admin/${screen}`, `ai-admin-${screen}.html`);
  page.on('dialog', (dialog) => dialog.accept());
  const writes = [];
  const financeQueries = [];
  const loadedPaths = [];
  const accountQueries = [];
  const stripeRecoveryWrites = [];
  await page.route('**/api/admin/billing/reconcile', (route) => {
    stripeRecoveryWrites.push(route.request().postDataJSON());
    return json(route, { reconciled: true });
  });
  const billingWrites = [];
  await page.route('**/api/admin/billing/users/**', (route) => {
    billingWrites.push(route.request().postDataJSON());
    return json(route, {
      billing_blocked: false,
      debt_micro: 0,
      writeoff_micro: 1000000,
    });
  });
  const user = {
    user_id: 'synthetic-user',
    name: 'Synthetic user',
    email: 'user@example.invalid',
    billing_blocked: true,
    debt_micro: 1000000,
    user_ai_enabled: false,
    managed_ai_allowed: false,
    byok_allowed: true,
    funding: 'none',
    access_request: 'pending',
    daily_limit_micro: 250000,
    revision: 4,
  };
  await page.route('**/api/admin/ai/**', (route) => {
    const path = new URL(route.request().url()).pathname;
    if (route.request().method() === 'GET') loadedPaths.push(path);
    if (route.request().method() !== 'GET') {
      writes.push({ path, body: route.request().postDataJSON() });
      return json(route, {});
    }
    if (path.endsWith('/users')) {
      accountQueries.push(new URL(route.request().url()).searchParams);
      return json(route, { users: [user] });
    }
    if (path.endsWith('/access-requests'))
      return json(route, { requests: [{ user_id: user.user_id, state: 'pending' }] });
    if (path.endsWith('/models'))
      return json(route, {
        models: [
          {
            ...model,
            classify: true,
            draft: true,
            margin_bps: -1,
            daily_limit_micro: 5000000,
          },
        ],
      });
    if (path.endsWith('/provider'))
      return json(route, {
        enabled: false,
        margin_bps: 2000,
        usd_eur_ppm: 900000,
        daily_limit_micro: 5000000,
        version: 3,
      });
    if (path.endsWith('/audit')) return json(route, { events: [] });
    financeQueries.push(new URL(route.request().url()).searchParams);
    return json(route, {
      topups_micro: 10000000,
      net_cash_collected_micro: 9750000,
      paid_outstanding_micro: 9500000,
      bad_debt_micro: 25000,
      margin_bps: 2500,
      usage_revenue_micro: 500000,
      provider_cost_micro: 300000,
      outstanding_micro: 9500000,
      payment_fees_micro: 50000,
      profit_micro: 125000,
      contribution_micro: 200000,
      models: [
        {
          model_id: model.id,
          provider: 'Synthetic provider',
          usage_revenue_micro: 500000,
          provider_cost_micro: 300000,
          contribution_micro: 200000,
          free_credits_used_micro: 0,
          free_inference_cost_micro: 0,
        },
      ],
    });
  });
  await page.goto('/admin/access');
  await expect(page.locator('[data-ai-billing]')).toHaveAttribute('aria-busy', 'false');
  await expect(page.locator('.ai-admin-user')).toHaveCount(0);
  expect(accountQueries).toHaveLength(0);
  const accountSearch = page.getByRole('combobox', { name: 'Find an account' });
  await accountSearch.fill('user@example');
  await expect(page.getByRole('option', { name: /Synthetic user/ })).toBeVisible();
  expect(accountQueries[0].get('q')).toBe('user@example');
  expect(accountQueries[0].get('limit')).toBe('20');
  await accountSearch.press('ArrowDown');
  await accountSearch.press('Enter');
  const access = page.locator('.ai-admin-user');
  await expect(access).toHaveCount(1);
  await access.getByLabel('Managed AI allowed').check();
  await access.getByLabel('Request decision').selectOption('approved');
  await access.getByLabel('Reason').fill('Synthetic approval');
  await access.getByRole('button', { name: 'Save access' }).click();
  await expect(page.locator('[data-ai-notice]')).toContainText('User access updated');
  expect(writes[0].body).toMatchObject({
    managed_ai_allowed: true,
    decision: 'approved',
    revision: 4,
    daily_limit_micro: 250000,
  });
  await page.goto('/admin/models');
  await expect(page.locator('[data-ai-billing]')).toHaveAttribute('aria-busy', 'false');
  await page.getByRole('button', { name: `Edit ${model.name}` }).click();
  const pricing = page.locator('.ai-admin-model');
  await pricing.getByLabel('Gross margin %').fill('20');
  await pricing.getByRole('button', { name: 'Save model' }).click();
  await expect(page.locator('[data-ai-notice]')).toContainText(
    'Existing quotes keep their accepted rates',
  );
  expect(writes[1].body).toMatchObject({ id: model.id, margin_bps: 2000, version: 1 });
  loadedPaths.length = 0;
  await page.goto('/admin/finance');
  await expect(page.locator('[data-ai-billing]')).toHaveAttribute('aria-busy', 'false');
  expect(loadedPaths).toEqual(['/api/admin/ai/finance']);
  const profit = page.locator('.ai-finance-headlines > div').filter({
    has: page.getByText('Profit', { exact: true }),
  });
  await expect(profit).toContainText('€0.125');
  await page.getByText('How profit is calculated', { exact: true }).click();
  const netMargin = page
    .locator('[data-admin-finance] dl > div')
    .filter({ has: page.getByText('Profit margin', { exact: true }) });
  await expect(netMargin).toContainText('25.00%');
  await expect(page.locator('[data-admin-model-finance]')).toContainText(
    'Synthetic provider',
  );
  await page.getByText('Filter by date', { exact: true }).click();
  const period = page.locator('[data-admin-finance-period]');
  await period.getByLabel('From (your timezone)').fill('2026-10-01T00:00');
  await period.getByLabel('To (your timezone)').fill('2026-10-31T23:59');
  await period.getByRole('button', { name: 'Apply finance period' }).click();
  await expect(page.locator('[data-ai-billing]')).toHaveAttribute('aria-busy', 'false');
  expect(financeQueries.at(-1).get('from')).toBe(
    await page.evaluate(() => String(new Date('2026-10-01T00:00').getTime() / 1000)),
  );
  expect(financeQueries.at(-1).get('to')).toBe(
    await page.evaluate(() => String(new Date('2026-10-31T23:59').getTime() / 1000)),
  );
  await page.goto('/admin/access');
  await expect(page.locator('[data-ai-billing]')).toHaveAttribute('aria-busy', 'false');
  await page.getByRole('button', { name: 'Review request' }).click();
  await page.getByRole('button', { name: 'View account spend' }).click();
  await expect(page.locator('[data-ai-notice]')).toContainText('Account spend loaded');
  expect(financeQueries.at(-1).get('user_id')).toBe(user.user_id);
  expect(financeQueries.at(-1).has('from')).toBe(false);
  const restriction = page.locator('.ai-admin-billing');
  await restriction.getByLabel('Block billing', { exact: true }).uncheck();
  await restriction.getByLabel('Forgive outstanding debt').check();
  await restriction
    .getByLabel('Billing action reason')
    .fill('Synthetic reviewed write-off');
  await restriction.getByRole('button', { name: 'Save billing restriction' }).click();
  await expect(page.locator('[data-ai-notice]')).toContainText(
    'Acknowledge the debt write-off',
  );
  expect(billingWrites).toHaveLength(0);
  await restriction.getByLabel('I acknowledge this debt write-off').check();
  await restriction.getByRole('button', { name: 'Save billing restriction' }).click();
  await expect(page.locator('[data-ai-notice]')).toContainText('debt write-off recorded');
  expect(billingWrites).toEqual([
    { blocked: false, forgive_debt: true, reason: 'Synthetic reviewed write-off' },
  ]);
  await page.goto('/admin/operations');
  await expect(page.locator('[data-ai-billing]')).toHaveAttribute('aria-busy', 'false');
  await page
    .getByText('Advanced: recover a missing Stripe payment', { exact: true })
    .click();
  const recovery = page.locator('[data-admin-stripe-reconcile]');
  await recovery.getByLabel('Local purchase ID').fill('synthetic-purchase');
  await recovery
    .getByLabel('Existing Stripe PaymentIntent ID')
    .fill('pi_synthetic_verified');
  await recovery
    .getByLabel('Reconciliation reason')
    .fill('Synthetic lost-response recovery');
  await recovery
    .getByRole('button', { name: 'Reconcile existing Stripe payment' })
    .click();
  await expect(page.locator('[data-ai-notice]')).toContainText(
    'No new charge was created',
  );
  expect(stripeRecoveryWrites).toEqual([
    {
      purchase_id: 'synthetic-purchase',
      payment_intent_id: 'pi_synthetic_verified',
      reason: 'Synthetic lost-response recovery',
    },
  ]);
});

test('managed usage distinguishes total credit use, free coverage and paid consumption', async ({
  page,
}) => {
  await shell(page, '/settings/ai/usage', 'ai-usage.html');
  await page.route('**/api/account/ai/usage?**', (route) =>
    json(route, {
      operations: [
        {
          id: 'synthetic-free-use',
          kind: 'draft',
          funding: 'credits',
          model_id: model.id,
          provider: 'Synthetic provider',
          status: 'settled',
          input_tokens: 30,
          output_tokens: 20,
          credit_debit_micro: 5000,
          free_credit_micro: 3000,
          customer_micro: 2000,
          currency: 'EUR',
          created_at: 1793275200,
        },
      ],
    }),
  );
  await page.goto('/settings/ai/usage');
  await expect(page.locator('[data-ai-billing]')).toHaveAttribute('aria-busy', 'false');
  await expect(page.locator('[data-usage-rows]')).toContainText('Credits used €0.005');
  await expect(page.locator('[data-usage-rows]')).toContainText('Free covered €0.003');
  await expect(page.locator('[data-usage-rows]')).toContainText('Paid consumed €0.002');
});

test('admin free grants require a reason and retain the exact reference after an uncertain response', async ({
  page,
}) => {
  await shell(page, '/admin/credits', 'ai-admin-credits.html');
  await shell(page, '/admin/finance', 'ai-admin-finance.html');
  const writes = [];
  const grants = new Map();
  await page.route('**/api/admin/ai/**', (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/grants')) {
      const payload = route.request().postDataJSON();
      writes.push(payload);
      if (!grants.has(payload.reference))
        grants.set(payload.reference, payload.amount_micro);
      if (writes.length === 1) return route.abort('failed');
      return json(route, { saved: true });
    }
    if (path.endsWith('/users'))
      return json(route, {
        users: [
          {
            user_id: 'synthetic-grantee',
            name: 'Synthetic recipient',
            email: 'recipient@example.invalid',
          },
        ],
      });
    if (path.endsWith('/access-requests')) return json(route, { requests: [] });
    if (path.endsWith('/models')) return json(route, { models: [] });
    if (path.endsWith('/provider'))
      return json(route, {
        enabled: false,
        margin_bps: 0,
        usd_eur_ppm: 0,
        daily_limit_micro: 5000000,
        version: 1,
      });
    if (path.endsWith('/audit')) return json(route, { entries: [] });
    const issued = [...grants.values()].reduce((sum, value) => sum + value, 0);
    return json(route, {
      topups_micro: 0,
      usage_revenue_micro: 0,
      provider_cost_micro: 50000,
      outstanding_micro: issued,
      contribution_micro: -50000,
      free_credits_issued_micro: issued,
      free_credits_used_micro: 0,
      free_credits_remaining_micro: issued,
      free_inference_cost_micro: 50000,
      models: [
        {
          model_id: model.id,
          usage_revenue_micro: 0,
          provider_cost_micro: 50000,
          contribution_micro: -50000,
          free_credits_used_micro: 0,
          free_inference_cost_micro: 50000,
        },
      ],
    });
  });
  await page.goto('/admin/credits');
  await expect(page.locator('[data-ai-billing]')).toHaveAttribute('aria-busy', 'false');
  const form = page.locator('[data-admin-grant]');
  const search = form.getByRole('combobox', { name: 'Find an account' });
  await search.fill('recipient@example');
  await form
    .getByRole('option', {
      name: 'Synthetic recipient · recipient@example.invalid · synthetic-grantee',
    })
    .click();
  await expect(form.locator('input[name=user_id]')).toHaveValue('synthetic-grantee');
  await form.getByLabel('Free credit amount (€)').fill('1.000001');
  const reference = await form.getByLabel('Grant tracking reference').inputValue();
  expect(reference).toMatch(/^grant-/);
  await expect(form.getByLabel('Grant tracking reference')).toHaveAttribute(
    'readonly',
    '',
  );
  await form.getByRole('button', { name: 'Grant free credits', exact: true }).click();
  expect(writes).toHaveLength(0);
  await form
    .getByLabel('Why are you granting these credits?')
    .fill('Synthetic support grant');
  await form.getByRole('button', { name: 'Grant free credits', exact: true }).click();
  await expect(page.locator('[data-ai-notice]')).toContainText('could not be confirmed');
  await form.getByRole('button', { name: 'Grant free credits', exact: true }).click();
  await expect(page.locator('[data-ai-notice]')).toContainText(
    'No payment or revenue was recorded',
  );
  expect(writes).toHaveLength(2);
  expect(writes[0]).toEqual(writes[1]);
  expect(writes[0]).toMatchObject({
    amount_micro: 1000001,
    reference,
    reason: 'Synthetic support grant',
  });
  await page.goto('/admin/finance');
  await expect(page.locator('[data-ai-billing]')).toHaveAttribute('aria-busy', 'false');
  await page.getByText('Free credit promotions', { exact: true }).click();
  await page.getByText('Payments and balances', { exact: true }).click();
  await page.getByText('How profit is calculated', { exact: true }).click();
  const issued = page
    .locator('[data-admin-finance] dl > div')
    .filter({ has: page.getByText('Free credits issued', { exact: true }) });
  await expect(issued).toContainText('€1.000001');
  const topups = page
    .locator('[data-admin-finance] dl > div')
    .filter({ has: page.getByText('Top-ups collected', { exact: true }) });
  await expect(topups).toContainText('€0.00');
  const revenue = page
    .locator('[data-admin-finance] dl > div')
    .filter({ has: page.getByText('Earned from AI usage', { exact: true }) });
  await expect(revenue).toContainText('€0.00');
  await expect(page.locator('[data-admin-model-finance]')).toContainText('€0.05');
});
