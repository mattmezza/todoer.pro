import { request } from './request.js';

const root = document.querySelector('[data-ai-billing]');
const csrf = document.querySelector('meta[name=csrf-token]')?.content || '';
const screen = root?.dataset.aiBilling;
let policy;
let models = [];
let billing;
let usageRows = [];
let usageBefore = '';
let nextBefore = '';
const purchaseStorageKey = `todoer:topup:${document.querySelector('meta[name=account-id]')?.content || 'account'}`;
let purchaseAttempt;
try {
  purchaseAttempt = JSON.parse(sessionStorage.getItem(purchaseStorageKey));
} catch {
  /* Storage can be disabled. */
}
let busy = false;
let providerConfig;
let noticeThrough = 0;
let adminUsersAfter = '';
let adminRequestsAfter = '';
let adminUsersNext = '';
let adminRequestsNext = '';
let adminAuditAfter = '';
let adminAuditNext = '';
let financeRange = {};

function element(tag, text, attrs = {}) {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  for (const [key, value] of Object.entries(attrs)) {
    if (key === 'checked') node.checked = Boolean(value);
    else node.setAttribute(key, String(value));
  }
  return node;
}
function notice(message, failed = false) {
  const node = root.querySelector('[data-ai-notice]');
  node.textContent = message;
  node.classList.toggle('danger-text', failed);
}
function list(data, key) {
  return Array.isArray(data) ? data : data[key] || [];
}
function money(value, currency = 'EUR', scale = 1000000) {
  if (value === null || value === undefined || !Number.isFinite(Number(value)))
    return 'Not available';
  return new Intl.NumberFormat(undefined, {
    style: 'currency',
    currency,
    minimumFractionDigits: 2,
    maximumFractionDigits: scale === 100 ? 2 : 6,
  }).format(Number(value) / scale);
}
function timestamp(value) {
  if (!value) return 'Not available';
  const parsed = typeof value === 'number' ? new Date(value * 1000) : new Date(value);
  return Number.isNaN(parsed.getTime()) ? 'Not available' : parsed.toLocaleString();
}
function cents(input) {
  const value = Number(input.value);
  if (!Number.isFinite(value)) throw Error('Enter a valid amount.');
  return Math.round(value * 100);
}
function field(form, name) {
  return form.elements.namedItem(name);
}
function metric(container, label, value) {
  const item = element('div');
  item.append(element('dt', label), element('dd', value));
  container.append(item);
}
async function api(path, body, method = 'GET') {
  const response = await request(path, {
    method,
    headers: {
      'Content-Type': 'application/json',
      'X-CSRF-Token': csrf,
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  const data = await response.json();
  if (!response.ok) {
    const error = Error(
      data.error?.message ||
        data.message ||
        (typeof data.error === 'string' ? data.error : 'The request failed.'),
    );
    error.code = data.error?.code || data.code;
    throw error;
  }
  return data;
}
async function run(callback) {
  if (busy) return;
  busy = true;
  root.setAttribute('aria-busy', 'true');
  try {
    await callback();
  } catch (error) {
    notice(error.message, true);
    if (['reauthentication_required', 'unauthorized'].includes(error.code)) {
      root.querySelector('[data-ai-notice]').append(
        document.createTextNode(' '),
        element('a', 'Sign in again to continue', {
          href: `/signin?next=${encodeURIComponent(location.pathname)}`,
        }),
      );
    }
  } finally {
    busy = false;
    root.setAttribute('aria-busy', 'false');
  }
}
function modelOptions() {
  for (const select of root.querySelectorAll('[data-ai-model-select]')) {
    select.replaceChildren(element('option', 'Choose a model', { value: '' }));
    for (const model of models.filter((item) => item.enabled)) {
      select.append(element('option', model.name || model.id, { value: model.id }));
    }
    select.value = policy.model_id || '';
  }
}
function renderPolicy() {
  const form = root.querySelector('[data-ai-policy]');
  field(form, 'user_ai_enabled').checked = policy.user_ai_enabled;
  field(form, 'funding').value = policy.funding;
  field(form, 'funding').querySelector('[value="credits"]').disabled =
    !policy.managed_ai_allowed;
  field(form, 'funding').querySelector('[value="byok"]').disabled =
    !policy.byok_allowed || !policy.key_configured;
  root.querySelector('[data-ai-entitlement]').textContent = policy.managed_ai_allowed
    ? 'AI credits access is approved. Add credits and choose Todoer credits to use it.'
    : `AI credits access is not enabled. Request status: ${policy.access_request || 'not requested'}.`;
  root.querySelector('[data-ai-request-access]').disabled =
    policy.managed_ai_allowed || policy.access_request === 'pending';
  root.querySelector('[data-ai-key-status]').textContent = !policy.byok_allowed
    ? 'BYOK is disabled by an administrator.'
    : `Key status: ${policy.key_status || 'absent'}.`;
  root.querySelector('[data-ai-key]').querySelector('button').disabled =
    !policy.byok_allowed;
  root.querySelector('[data-ai-remove-key]').disabled = !policy.key_configured;
  modelOptions();
  modelPrice();
}
function modelPrice() {
  const selected = root.querySelector('[data-ai-policy] [name=model_id]').value;
  const model = models.find((item) => item.id === selected);
  root.querySelector('[data-ai-model-price]').textContent = model
    ? `OpenRouter · ${model.name || model.id}. Managed rates per million tokens: input ${money(model.input_micro_per_million)}, output ${money(model.output_micro_per_million)}. BYOK is billed by OpenRouter at its applicable rates.`
    : 'No supported model selected.';
}
async function loadAI() {
  let notices;
  [policy, { models }, notices] = await Promise.all([
    api('/api/account/ai/settings'),
    api('/api/account/ai/models'),
    api('/api/account/ai/notices'),
  ]);
  const updates = root.querySelector('[data-ai-notices]');
  updates.replaceChildren();
  for (const item of notices.notices || []) {
    updates.append(
      element(
        'p',
        `${item.unread ? 'New · ' : ''}${item.kind}: ${item.state} · ${timestamp(item.created_at)}`,
      ),
    );
    noticeThrough = Math.max(noticeThrough, Number(item.id) || 0);
  }
  if (!notices.notices?.length) updates.append(element('p', 'No account AI updates.'));
  root.querySelector('[data-ai-notices-read]').disabled = !notices.notices?.some(
    (item) => item.unread,
  );
  renderPolicy();
  notice('AI settings loaded. Changes apply to new requests.');
}
function bindAI() {
  root.querySelector('[data-ai-notices-read]').addEventListener('click', () =>
    run(async () => {
      await api('/api/account/ai/notices', { through: noticeThrough }, 'POST');
      await loadAI();
    }),
  );
  root
    .querySelector('[data-ai-policy] [name=model_id]')
    .addEventListener('change', modelPrice);
  root.querySelector('[data-ai-policy]').addEventListener('submit', (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    run(async () => {
      await api(
        '/api/account/ai/settings',
        {
          user_ai_enabled: field(form, 'user_ai_enabled').checked,
          funding: field(form, 'funding').value,
          model_id: field(form, 'model_id').value || null,
          revision: policy.revision,
        },
        'PUT',
      );
      await loadAI();
      notice('AI settings saved.');
    });
  });
  root.querySelector('[data-ai-request-access]').addEventListener('click', () =>
    run(async () => {
      await api('/api/account/ai/access-request', {}, 'POST');
      await loadAI();
      notice('Access request sent. An administrator will review it.');
    }),
  );
  root.querySelector('[data-ai-key]').addEventListener('submit', (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    if (busy || !policy) return;
    const key = field(form, 'key').value;
    field(form, 'key').value = '';
    run(async () => {
      await api(
        '/api/account/ai/key',
        { key, model_id: field(form, 'model_id').value, revision: policy.revision },
        'PUT',
      );
      await loadAI();
      notice(
        'OpenRouter key saved. BYOK funding selected; your AI switch was preserved.',
      );
    });
  });
  root.querySelector('[data-ai-remove-key]').addEventListener('click', () =>
    run(async () => {
      await api('/api/account/ai/key', undefined, 'DELETE');
      await loadAI();
      notice('Key removed. No fallback to Todoer credits was enabled.');
    }),
  );
}
function currencyRate(code) {
  return (
    billing.currencies.find((item) => item.code === code)?.eur_rate_micros || 1000000
  );
}
function showTopupEquivalent() {
  const form = root.querySelector('[data-credit-topup]');
  const code = field(form, 'currency').value;
  const rate = currencyRate(code);
  field(form, 'amount').min = (Math.ceil((500 * rate) / 1000000) / 100).toFixed(2);
  root.querySelector('[data-topup-equivalent]').textContent =
    `Minimum ${money(Math.ceil((500 * rate) / 1000000), code, 100)} (€5 equivalent). Your wallet is denominated in EUR. Checkout confirms the payment.`;
}
function renderBilling() {
  const wallet = root.querySelector('[data-wallet]');
  wallet.replaceChildren();
  metric(wallet, 'Posted balance', money(billing.wallet.posted_micro));
  metric(wallet, 'Reserved', money(billing.wallet.held_micro));
  metric(wallet, 'Available', money(billing.wallet.available_micro));
  metric(wallet, 'Paid credits', money(billing.wallet.paid_micro));
  metric(wallet, 'Free credits', money(billing.wallet.free_micro));
  const topup = root.querySelector('[data-credit-topup]');
  const currency = field(topup, 'currency');
  const selected = currency.value;
  currency.replaceChildren(
    ...billing.currencies.map((item) =>
      element('option', item.code, { value: item.code }),
    ),
  );
  currency.value = billing.currencies.some((item) => item.code === selected)
    ? selected
    : 'EUR';
  topup.querySelector('button[type=submit]').disabled = !billing.enabled;
  showTopupEquivalent();
  const auto = billing.auto_topup;
  const form = root.querySelector('[data-auto-topup]');
  field(form, 'enabled').checked = auto.enabled;
  field(form, 'amount').value = (auto.amount_minor / 100).toFixed(2);
  field(form, 'threshold').value = (auto.threshold_minor / 100).toFixed(2);
  field(form, 'monthly_cap').value = (auto.monthly_cap_minor / 100).toFixed(2);
  field(form, 'cooldown_hours').value = auto.cooldown_seconds / 3600;
  field(form, 'consent').checked = false;
  root.querySelector('[data-auto-state]').textContent =
    `${auto.enabled ? 'Enabled' : 'Disabled'}${auto.paused_reason ? ` · Paused: ${auto.paused_reason}` : ''}. ${auto.payment_method_configured ? 'Payment method is saved.' : 'Save a payment method during a top-up before enabling this.'}`;
  if (['authentication_required', 'auth_required'].includes(auto.paused_reason))
    root.querySelector('[data-auto-state]').textContent +=
      ' Complete a manual top-up with payment-method saving checked, then explicitly re-enable auto-top-up. The previous unconfirmed payment will not be retried automatically.';
  const rows = root.querySelector('[data-purchases]');
  rows.replaceChildren();
  for (const purchase of billing.purchases) {
    const tr = element('tr');
    for (const value of [
      timestamp(purchase.created_at),
      money(purchase.amount_minor, purchase.currency.toUpperCase(), 100),
      money(purchase.credit_micro),
      purchase.status,
    ])
      tr.append(element('td', value));
    rows.append(tr);
  }
  if (!billing.purchases.length) emptyRow(rows, 4, 'No top-ups yet.');
}
async function loadBilling() {
  billing = await api('/api/account/billing');
  billing.currencies ||= [{ code: 'EUR', eur_rate_micros: 1000000 }];
  renderBilling();
  notice(
    billing.enabled
      ? 'Billing loaded.'
      : 'Credit purchases are not enabled on this instance.',
  );
}
function bindBilling() {
  const topup = root.querySelector('[data-credit-topup]');
  field(topup, 'currency').addEventListener('change', showTopupEquivalent);
  root.querySelectorAll('[data-topup-preset]').forEach((button) =>
    button.addEventListener('click', () => {
      field(topup, 'amount').value = (
        Math.ceil(
          (Number(button.dataset.topupPreset) *
            100 *
            currencyRate(field(topup, 'currency').value)) /
            1000000,
        ) / 100
      ).toFixed(2);
    }),
  );

  topup.addEventListener('submit', (event) => {
    event.preventDefault();
    run(async () => {
      const payload = {
        amount_minor: cents(field(topup, 'amount')),
        currency: field(topup, 'currency').value,
        save_for_auto_topup: field(topup, 'save_for_auto_topup').checked,
      };
      const fingerprint = JSON.stringify(payload);
      if (!purchaseAttempt || purchaseAttempt.fingerprint !== fingerprint)
        purchaseAttempt = { fingerprint, key: crypto.randomUUID() };
      try {
        sessionStorage.setItem(purchaseStorageKey, JSON.stringify(purchaseAttempt));
      } catch {
        /* In-memory retries still reuse the same request key. */
      }
      const result = await api(
        '/api/account/billing/topups',
        { ...payload, idempotency_key: purchaseAttempt.key },
        'POST',
      );
      if (result.checkout_url) {
        const target = new URL(result.checkout_url);
        if (target.protocol !== 'https:' || target.hostname !== 'checkout.stripe.com')
          throw Error(
            'The payment response did not contain a valid Stripe Checkout link.',
          );
        location.assign(target.href);
      } else {
        await loadBilling();
        notice(
          `Payment status: ${result.status || 'pending'}. Credits are added only after payment confirmation.`,
        );
      }
    });
  });
  root.querySelector('[data-auto-topup]').addEventListener('submit', (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    run(async () => {
      await api(
        '/api/account/billing/auto-topup',
        {
          enabled: field(form, 'enabled').checked,
          amount_minor: cents(field(form, 'amount')),
          threshold_minor: cents(field(form, 'threshold')),
          monthly_cap_minor: cents(field(form, 'monthly_cap')),
          cooldown_seconds: Number(field(form, 'cooldown_hours').value) * 3600,
          consent: field(form, 'consent').checked,
        },
        'PUT',
      );
      await loadBilling();
      notice('Auto-top-up settings saved.');
    });
  });
}
function emptyRow(container, columns, message) {
  const tr = element('tr');
  tr.append(element('td', message, { colspan: columns }));
  container.append(tr);
}
function usageValues(row) {
  const tokens = `${row.input_tokens ?? '?'} / ${row.output_tokens ?? '?'}`;
  const details = [
    row.cached_tokens == null ? '' : `cached ${row.cached_tokens}`,
    row.reasoning_tokens == null ? '' : `reasoning ${row.reasoning_tokens}`,
  ]
    .filter(Boolean)
    .join(', ');
  const charge =
    row.funding === 'byok'
      ? `Todoer ${money(0)} · OpenRouter ${money(row.byok_cost_micro_usd, 'USD')}`
      : `Credits used ${money(row.credit_debit_micro, row.currency || 'EUR')} · Free covered ${money(row.free_credit_micro, row.currency || 'EUR')} · Paid consumed ${money(row.customer_micro, row.currency || 'EUR')}`;
  return [
    timestamp(row.created_at),
    row.kind,
    row.funding === 'byok' ? 'BYOK' : 'Todoer credits',
    `${row.actual_model || row.model_id || 'Unknown model'} / ${row.provider || 'Unknown provider'}`,
    `${tokens}${details ? ` (${details})` : ''}`,
    charge,
    row.status,
  ];
}
async function loadUsage() {
  const filters = root.querySelector('[data-usage-filter]');
  const params = new URLSearchParams({ limit: '50' });
  for (const name of ['funding', 'status'])
    if (field(filters, name).value) params.set(name, field(filters, name).value);
  if (usageBefore) params.set('before', usageBefore);
  const data = await api(`/api/account/ai/usage?${params}`);
  usageRows = list(data, 'operations');
  nextBefore = data.next_before || '';
  root.querySelector('[data-usage-next]').hidden = !nextBefore;
  const rows = root.querySelector('[data-usage-rows]');
  rows.replaceChildren();
  for (const row of usageRows) {
    const tr = element('tr');
    for (const value of usageValues(row)) tr.append(element('td', value));
    rows.append(tr);
  }
  if (!usageRows.length) emptyRow(rows, 7, 'No matching AI transactions.');
  notice('AI transactions loaded. Times use your browser timezone.');
}
function exportUsage() {
  const escape = (value) => {
    const text = String(value ?? '');
    const safe = /^[=+\-@\t\r]/.test(text) ? `'${text}` : text;
    return `"${safe.replaceAll('"', '""')}"`;
  };
  const rows = [
    [
      'Time',
      'Feature',
      'Funding',
      'Model / provider',
      'Input / output tokens',
      'Credit use / paid charge',
      'Status',
    ],
    ...usageRows.map(usageValues),
  ];
  const blob = new Blob([rows.map((row) => row.map(escape).join(',')).join('\r\n')], {
    type: 'text/csv;charset=utf-8',
  });
  const url = URL.createObjectURL(blob);
  const link = element('a', '', { href: url, download: 'todoer-ai-usage.csv' });
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
function bindUsage() {
  root.querySelector('[data-usage-filter]').addEventListener('submit', (event) => {
    event.preventDefault();
    usageBefore = '';
    run(loadUsage);
  });
  root.querySelector('[data-usage-next]').addEventListener('click', () => {
    usageBefore = nextBefore;
    run(loadUsage);
  });
  root.querySelector('[data-usage-export]').addEventListener('click', exportUsage);
}
function inputLabel(label, name, value, type = 'text') {
  const node = element('label', label);
  const input = element('input', undefined, {
    name,
    type,
    ...(type === 'checkbox' ? { checked: Boolean(value) } : { value: value ?? '' }),
  });
  if (type === 'number') input.step = '1';
  node.append(input);
  return node;
}
function renderUsers(users, requests) {
  const queue = root.querySelector('[data-admin-requests]');
  queue.replaceChildren();
  for (const item of requests)
    queue.append(
      element('p', `${item.user_id} · ${item.state} · ${timestamp(item.requested_at)}`),
    );
  if (!requests.length) queue.append(element('p', 'No access requests.'));
  const container = root.querySelector('[data-admin-users]');
  container.replaceChildren();
  for (const user of users) {
    const form = element('form', undefined, { class: 'ai-admin-user' });
    form.append(
      element('h3', user.user_id),
      element(
        'p',
        `AI switch: ${user.user_ai_enabled ? 'on' : 'off'} · Funding: ${user.funding} · Request: ${user.access_request || 'none'}`,
      ),
      inputLabel(
        'Managed AI allowed',
        'managed_ai_allowed',
        user.managed_ai_allowed,
        'checkbox',
      ),
      inputLabel('BYOK allowed', 'byok_allowed', user.byok_allowed, 'checkbox'),
      inputLabel(
        'Daily spend limit (micro-EUR)',
        'daily_limit_micro',
        user.daily_limit_micro,
        'number',
      ),
    );
    const decision = element('label', 'Request decision');
    const select = element('select', undefined, { name: 'decision' });
    for (const [value, label] of [
      ['', 'Leave request unchanged'],
      ['approved', 'Approve'],
      ['denied', 'Deny'],
      ['revoked', 'Revoke'],
    ])
      select.append(element('option', label, { value }));
    decision.append(select);
    form.append(
      decision,
      inputLabel('Reason', 'reason', ''),
      element('button', 'Save access', { type: 'submit', class: 'secondary' }),
    );
    const spend = element('button', 'View account spend', {
      type: 'button',
      class: 'secondary',
    });
    const spendMetrics = element('dl', undefined, { class: 'ai-metrics' });
    spend.addEventListener('click', () =>
      run(async () => {
        const report = await api(financeURL(user.user_id));
        spendMetrics.replaceChildren();
        for (const [label, key] of [
          ['Top-ups', 'topups_micro'],
          ['Available credits', 'available_micro'],
          ['Held credits', 'held_micro'],
          ['Paid-credit usage revenue', 'usage_revenue_micro'],
          ['Free credits issued', 'free_credits_issued_micro'],
          ['Free credits used', 'free_credits_used_micro'],
          ['Free credits remaining', 'free_credits_remaining_micro'],
          ['Inference expense covered by free credits', 'free_inference_cost_micro'],
          ['Provider expense', 'provider_cost_micro'],
          ['Payment fees', 'payment_fees_micro'],
          ['Contribution before payment fees (provisional)', 'contribution_micro'],
        ])
          metric(spendMetrics, label, money(report[key]));
        netFinanceMetrics(spendMetrics, report);
        notice('Account spend loaded.');
      }),
    );
    form.append(spend, spendMetrics);
    field(form, 'reason').required = true;
    form.addEventListener('submit', (event) => {
      event.preventDefault();
      run(async () => {
        await api(
          `/api/admin/ai/users/${encodeURIComponent(user.user_id)}`,
          {
            managed_ai_allowed: field(form, 'managed_ai_allowed').checked,
            daily_limit_micro: Number(field(form, 'daily_limit_micro').value),
            byok_allowed: field(form, 'byok_allowed').checked,
            decision: field(form, 'decision').value || null,
            reason: field(form, 'reason').value,
            revision: user.revision,
          },
          'PUT',
        );
        await loadAdmin();
        notice('User access updated.');
      });
    });
    container.append(form);
    const billingForm = element('form', undefined, { class: 'ai-admin-billing' });
    billingForm.append(
      element('h4', `Billing restrictions · ${user.user_id}`),
      element(
        'p',
        `Billing ${user.billing_blocked ? 'blocked' : 'not blocked'} · Unpaid debt ${money(user.debt_micro)}.`,
      ),
      inputLabel('Block billing', 'blocked', user.billing_blocked, 'checkbox'),
      inputLabel('Forgive outstanding debt', 'forgive_debt', false, 'checkbox'),
      element(
        'p',
        'Forgiving debt records a write-off. It does not refund a payment or erase usage history. Changing the billing block is a separate explicit choice.',
      ),
      inputLabel(
        'I acknowledge this debt write-off',
        'acknowledge_writeoff',
        false,
        'checkbox',
      ),
      inputLabel('Billing action reason', 'billing_reason', ''),
      element('button', 'Save billing restriction', {
        type: 'submit',
        class: 'secondary',
      }),
    );
    field(billingForm, 'billing_reason').required = true;
    billingForm.addEventListener('submit', (event) => {
      event.preventDefault();
      run(async () => {
        const forgive = field(billingForm, 'forgive_debt').checked;
        if (forgive && !field(billingForm, 'acknowledge_writeoff').checked)
          throw Error('Acknowledge the debt write-off before forgiving debt.');
        await api(
          `/api/admin/billing/users/${encodeURIComponent(user.user_id)}`,
          {
            blocked: field(billingForm, 'blocked').checked,
            forgive_debt: forgive,
            reason: field(billingForm, 'billing_reason').value,
          },
          'PUT',
        );
        await loadAdmin();
        notice(
          forgive
            ? 'Billing restriction saved and debt write-off recorded.'
            : 'Billing restriction saved.',
        );
      });
    });
    container.append(billingForm);
  }
}
function renderAdminModels(items) {
  const container = root.querySelector('[data-admin-models]');
  container.replaceChildren();
  for (const model of items) {
    const form = element('form', undefined, { class: 'ai-admin-model' });
    form.append(
      element('h3', model.name || model.id),
      element('p', model.id),
      inputLabel('Display name', 'name', model.name),
      inputLabel('Enabled', 'enabled', model.enabled, 'checkbox'),
      inputLabel('Classification supported', 'classify', model.classify, 'checkbox'),
      inputLabel('Drafting supported', 'draft', model.draft, 'checkbox'),
      inputLabel(
        'Source input rate (micro-EUR / million tokens)',
        'input_micro_per_million',
        model.input_micro_per_million,
        'number',
      ),
      inputLabel(
        'Source output rate (micro-EUR / million tokens)',
        'output_micro_per_million',
        model.output_micro_per_million,
        'number',
      ),
      inputLabel(
        'Source cached-input rate (micro-EUR / million tokens)',
        'cached_micro_per_million',
        model.cached_micro_per_million,
        'number',
      ),
      inputLabel(
        'Gross margin basis points (-1 inherits provider)',
        'margin_bps',
        model.margin_bps,
        'number',
      ),
      inputLabel(
        'Daily spend limit (micro-EUR)',
        'daily_limit_micro',
        model.daily_limit_micro,
        'number',
      ),
      element('button', 'Save future model pricing', {
        type: 'submit',
        class: 'secondary',
      }),
    );
    form.addEventListener('submit', (event) => {
      event.preventDefault();
      run(async () => {
        const value = {
          id: model.id,
          name: field(form, 'name').value,
          enabled: field(form, 'enabled').checked,
          classify: field(form, 'classify').checked,
          draft: field(form, 'draft').checked,
          version: model.version,
        };
        for (const name of [
          'input_micro_per_million',
          'output_micro_per_million',
          'cached_micro_per_million',
          'margin_bps',
          'daily_limit_micro',
        ])
          value[name] = Number(field(form, name).value);
        await api('/api/admin/ai/models', value, 'PUT');
        await loadAdmin();
        notice(
          'Future model pricing updated. Existing quotes keep their accepted rates.',
        );
      });
    });
    container.append(form);
  }
  if (!items.length) container.append(element('p', 'No models configured.'));
}
function financeURL(userId) {
  const params = new URLSearchParams(financeRange);
  if (userId) params.set('user_id', userId);
  const query = params.toString();
  return `/api/admin/ai/finance${query ? `?${query}` : ''}`;
}
function netFinanceMetrics(container, data) {
  for (const [label, key] of [
    ['Net cash collected', 'net_cash_collected_micro'],
    ['Paid unused credits (current)', 'paid_outstanding_micro'],
    ['Bad debt', 'bad_debt_micro'],
    ['Profit after provider/payment costs and adjustments', 'profit_micro'],
  ])
    metric(container, label, money(data[key]));
  metric(
    container,
    'Net margin',
    data.margin_bps == null ? 'Not available' : `${(data.margin_bps / 100).toFixed(2)}%`,
  );
  if (data.profit_micro == null || data.provisional)
    container.append(
      element(
        'p',
        'Net profit is provisional or unavailable until provider expense and payment fees are reconciled. Unknown amounts are not zero.',
      ),
    );
}
function renderFinance(data) {
  const container = root.querySelector('[data-admin-finance]');
  container.replaceChildren();
  for (const [label, key] of [
    ['Top-ups collected', 'topups_micro'],
    ['Paid-credit usage revenue', 'usage_revenue_micro'],
    ['Free credits issued', 'free_credits_issued_micro'],
    ['Free credits used', 'free_credits_used_micro'],
    ['Free credits remaining', 'free_credits_remaining_micro'],
    ['Inference expense covered by free credits', 'free_inference_cost_micro'],
    ['Provider expense', 'provider_cost_micro'],
    ['Unused credits', 'outstanding_micro'],
    ['Payment fees', 'payment_fees_micro'],
    ['Contribution before payment fees (provisional)', 'contribution_micro'],
  ])
    metric(container, label, money(data[key]));
  metric(
    container,
    'BYOK requests (no Todoer revenue or provider expense)',
    data.byok_calls == null ? 'Not available' : String(data.byok_calls),
  );
  netFinanceMetrics(container, data);
  metric(
    container,
    'Contribution margin before fees (provisional)',
    data.usage_revenue_micro > 0 && data.contribution_micro != null
      ? `${((100 * data.contribution_micro) / data.usage_revenue_micro).toFixed(2)}%`
      : 'Not available',
  );
  if (data.provider_fx_unknown)
    container.append(
      element(
        'p',
        'Provider currency conversion is incomplete; contribution is provisional.',
      ),
    );
  const rows = root.querySelector('[data-admin-model-finance]');
  rows.replaceChildren();
  for (const row of data.models || []) {
    const tr = element('tr');
    for (const value of [
      `${row.model_id || row.id} / ${row.provider || 'Unknown provider'}`,
      money(row.usage_revenue_micro),
      money(row.provider_cost_micro),
      money(row.contribution_micro),
      row.usage_revenue_micro > 0 && row.contribution_micro != null
        ? `${((100 * row.contribution_micro) / row.usage_revenue_micro).toFixed(2)}%`
        : 'Not available',
      money(row.free_credits_used_micro),
      money(row.free_inference_cost_micro),
    ])
      tr.append(element('td', value));
    rows.append(tr);
  }
  if (!data.models?.length) emptyRow(rows, 7, 'No model usage yet.');
}
async function loadAdmin() {
  const [users, requests, configured, finance, provider, audit] = await Promise.all([
    api(`/api/admin/ai/users?limit=50&after=${encodeURIComponent(adminUsersAfter)}`),
    api(
      `/api/admin/ai/access-requests?limit=50&after=${encodeURIComponent(adminRequestsAfter)}`,
    ),
    api('/api/admin/ai/models'),
    api(financeURL()),
    api('/api/admin/ai/provider'),
    api(
      `/api/admin/ai/audit?limit=25${adminAuditAfter ? `&after=${encodeURIComponent(adminAuditAfter)}` : ''}`,
    ),
  ]);
  adminUsersNext = users.next_cursor || '';
  adminRequestsNext = requests.next_cursor || '';
  adminAuditNext = audit.next_cursor || '';
  root.querySelector('[data-admin-audit-next]').hidden = !adminAuditNext;
  root.querySelector('[data-admin-users-next]').hidden = !adminUsersNext;
  root.querySelector('[data-admin-requests-next]').hidden = !adminRequestsNext;
  const auditContainer = root.querySelector('[data-admin-audit]');
  auditContainer.replaceChildren();
  for (const event of audit.entries || audit.events || audit.audit || [])
    auditContainer.append(
      element(
        'p',
        `${timestamp(event.created_at)} · ${event.action || event.kind} · ${event.target || event.target_user_id || event.user_id || ''}`,
      ),
    );
  providerConfig = provider;
  const form = root.querySelector('[data-admin-provider]');
  field(form, 'enabled').checked = provider.enabled;
  for (const name of ['margin_bps', 'usd_eur_ppm', 'daily_limit_micro'])
    field(form, name).value = provider[name];
  renderUsers(list(users, 'users'), list(requests, 'requests'));
  renderAdminModels(list(configured, 'models'));
  renderFinance(finance);
  notice('Admin controls loaded. Changes are recorded in the audit history.');
}
function bindAdmin() {
  root
    .querySelector('[data-admin-stripe-reconcile]')
    .addEventListener('submit', (event) => {
      event.preventDefault();
      const form = event.currentTarget;
      run(async () => {
        await api(
          '/api/admin/billing/reconcile',
          {
            purchase_id: field(form, 'purchase_id').value,
            payment_intent_id: field(form, 'payment_intent_id').value,
            reason: field(form, 'reason').value,
          },
          'POST',
        );
        await loadAdmin();
        notice('Existing Stripe payment reconciled. No new charge was created.');
      });
    });
  root
    .querySelector('[data-admin-finance-period]')
    .addEventListener('submit', (event) => {
      event.preventDefault();
      const form = event.currentTarget;
      run(async () => {
        const next = {};
        for (const name of ['from', 'to']) {
          const raw = field(form, name).value;
          if (!raw) continue;
          const value = Math.floor(new Date(raw).getTime() / 1000);
          if (!Number.isFinite(value) || value < 0)
            throw Error('Enter a valid finance period.');
          next[name] = String(value);
        }
        if (next.from && next.to && Number(next.from) > Number(next.to))
          throw Error('The finance period must end after it starts.');
        financeRange = next;
        await loadAdmin();
      });
    });
  root.querySelector('[data-admin-grant]').addEventListener('submit', (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    run(async () => {
      const raw = field(form, 'amount').value;
      if (!/^\d+(?:\.\d{1,6})?$/.test(raw) || raw.length > 20)
        throw Error('Use an amount with at most six decimal places.');
      const parts = raw.split('.');
      const amount =
        BigInt(parts[0]) * 1000000n + BigInt((parts[1] || '').padEnd(6, '0'));
      if (amount <= 0n || amount > 1000000000n)
        throw Error(
          'Enter a free credit amount above zero and no greater than EUR 1000.',
        );
      await api(
        '/api/admin/ai/grants',
        {
          user_id: field(form, 'user_id').value,
          amount_micro: Number(amount),
          reference: field(form, 'reference').value,
          reason: field(form, 'reason').value,
        },
        'POST',
      );
      await loadAdmin();
      notice('Free credit grant recorded. No payment or revenue was recorded.');
    });
  });
  root.querySelector('[data-admin-audit-next]').addEventListener('click', () => {
    adminAuditAfter = adminAuditNext;
    run(loadAdmin);
  });
  root.querySelector('[data-admin-users-next]').addEventListener('click', () => {
    adminUsersAfter = adminUsersNext;
    run(loadAdmin);
  });
  root.querySelector('[data-admin-users-first]').addEventListener('click', () => {
    adminUsersAfter = '';
    run(loadAdmin);
  });
  root.querySelector('[data-admin-requests-next]').addEventListener('click', () => {
    adminRequestsAfter = adminRequestsNext;
    run(loadAdmin);
  });
  root.querySelector('[data-admin-rotate]').addEventListener('click', () =>
    run(async () => {
      const result = await api('/api/admin/ai/keys/rotate', {}, 'POST');
      await loadAdmin();
      notice(
        `Rotated ${result.rotated} keys using the configured encryption keys. Rotation is bounded; repeat for remaining keys if necessary.`,
      );
    }),
  );
  root.querySelector('[data-admin-provider]').addEventListener('submit', (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    run(async () => {
      const value = {
        enabled: field(form, 'enabled').checked,
        version: providerConfig.version,
      };
      for (const name of ['margin_bps', 'usd_eur_ppm', 'daily_limit_micro'])
        value[name] = Number(field(form, name).value);
      await api('/api/admin/ai/provider', value, 'PUT');
      await loadAdmin();
      notice('Provider controls saved.');
    });
  });
  root.querySelector('[data-admin-adjustment]').addEventListener('submit', (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    run(async () => {
      await api(
        '/api/admin/ai/adjustments',
        {
          user_id: field(form, 'user_id').value,
          amount_micro: Math.round(Number(field(form, 'amount').value) * 1000000),
          reference: field(form, 'reference').value,
          reason: field(form, 'reason').value,
        },
        'POST',
      );
      await loadAdmin();
      notice('Credit adjustment recorded.');
    });
  });
  root.querySelector('[data-admin-reconcile]').addEventListener('click', () =>
    run(async () => {
      const result = await api('/api/admin/ai/reconcile', {}, 'POST');
      await loadAdmin();
      notice(
        `Reconciliation completed${result.released == null ? '.' : `: ${result.released} expired holds released.`}`,
      );
    }),
  );
}

if (root) {
  const loaders = {
    ai: loadAI,
    billing: loadBilling,
    usage: loadUsage,
    admin: loadAdmin,
  };
  const binders = {
    ai: bindAI,
    billing: bindBilling,
    usage: bindUsage,
    admin: bindAdmin,
  };
  binders[screen]?.();
  root
    .querySelector('[data-ai-reload]')
    ?.addEventListener('click', () => run(loaders[screen]));
  run(loaders[screen]);
}
