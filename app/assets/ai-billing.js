import { request } from './request.js';
import { toast } from './toast.js';
import { beginLoading } from './loading.js';

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
let actionTrigger;
let initialized = false;
let providerConfig;
let noticeThrough = 0;
let selectedAdminAccount = null;
let adminAccessRequests = [];
let accountPickerSequence = 0;
let adminRequestsAfter = '';
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
  const trigger = actionTrigger;
  actionTrigger = undefined;
  const priorContent = trigger ? [...trigger.childNodes] : [];
  const priorDisabled = trigger?.getAttribute('aria-disabled');
  const loading = beginLoading(root, { variant: 'cards', preserve: initialized });
  if (trigger) {
    trigger.textContent = trigger.matches('[data-ai-reload]')
      ? 'Refreshing…'
      : trigger.type === 'submit'
        ? 'Saving…'
        : 'Working…';
    trigger.setAttribute('aria-disabled', 'true');
    trigger.setAttribute('aria-busy', 'true');
    trigger.classList.add('is-saving');
  }
  try {
    await callback();
    initialized = true;
    if (trigger) toast(root.querySelector('[data-ai-notice]')?.textContent || 'Updated.');
  } catch (error) {
    if (!initialized) loading.fail(error.message, () => run(callback));
    notice(error.message, true);
    toast(error.message);
    if (['reauthentication_required', 'unauthorized'].includes(error.code)) {
      root.querySelector('[data-ai-notice]').append(
        document.createTextNode(' '),
        element('a', 'Sign in again to continue', {
          href: `/signin?next=${encodeURIComponent(location.pathname)}`,
        }),
      );
    }
  } finally {
    loading.finish();
    if (trigger?.isConnected) {
      trigger.replaceChildren(...priorContent);
      if (priorDisabled === null) trigger.removeAttribute('aria-disabled');
      else trigger.setAttribute('aria-disabled', priorDisabled);
      trigger.removeAttribute('aria-busy');
      trigger.classList.remove('is-saving');
    }
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
  const enabled = field(form, 'user_ai_enabled');
  if (enabled) enabled.checked = policy.user_ai_enabled;
  const funding = field(form, 'funding');
  if (funding) {
    funding.value = policy.funding;
    funding.querySelector('[value="credits"]').disabled = !policy.managed_ai_allowed;
    funding.querySelector('[value="byok"]').disabled =
      !policy.byok_allowed || !policy.key_configured;
  }
  const entitlement = root.querySelector('[data-ai-entitlement]');
  if (entitlement)
    entitlement.textContent = policy.managed_ai_allowed
      ? 'Credit-funded AI is ready. Add credits in Billing, then choose Todoer credits above.'
      : `Credit-funded AI needs approval. Request status: ${policy.access_request || 'not requested'}.`;
  const requestAccess = root.querySelector('[data-ai-request-access]');
  if (requestAccess)
    requestAccess.disabled =
      policy.managed_ai_allowed || policy.access_request === 'pending';
  const keyStatus = root.querySelector('[data-ai-key-status]');
  if (keyStatus)
    keyStatus.textContent = !policy.byok_allowed
      ? 'Using your own key is disabled by an administrator.'
      : policy.key_configured
        ? 'Your OpenRouter key is connected.'
        : 'No OpenRouter key connected.';
  const keyForm = root.querySelector('[data-ai-key]');
  if (keyForm) keyForm.querySelector('button').disabled = !policy.byok_allowed;
  const removeKey = root.querySelector('[data-ai-remove-key]');
  if (removeKey) removeKey.disabled = !policy.key_configured;
  modelOptions();
  modelPrice();
}
function modelPrice() {
  const select = root.querySelector('[data-ai-policy] [name=model_id]');
  const price = root.querySelector('[data-ai-model-price]');
  if (!select || !price) return;
  const model = models.find((item) => item.id === select.value);
  price.textContent = model
    ? `OpenRouter · ${model.name || model.id}. Credit rates per million tokens: input ${money(model.input_micro_per_million)}, output ${money(model.output_micro_per_million)}. Your own key is billed by OpenRouter.`
    : 'Choose a supported model to see its rates.';
}
async function loadAI() {
  const setup = root.dataset.aiSection === 'setup';
  let notices;
  [policy, { models }, notices] = await Promise.all([
    api('/api/account/ai/settings'),
    setup ? api('/api/account/ai/models') : { models: [] },
    setup ? api('/api/account/ai/notices') : { notices: [] },
  ]);
  const updates = root.querySelector('[data-ai-notices]');
  if (updates) {
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
  }
  const readUpdates = root.querySelector('[data-ai-notices-read]');
  if (readUpdates) readUpdates.disabled = !notices.notices?.some((item) => item.unread);
  renderPolicy();
  notice(setup ? 'AI setup loaded.' : 'AI preferences loaded.');
}
function bindAI() {
  root.querySelector('[data-ai-notices-read]')?.addEventListener('click', () =>
    run(async () => {
      await api('/api/account/ai/notices', { through: noticeThrough }, 'POST');
      await loadAI();
      notice('Updates marked as read.');
    }),
  );
  root
    .querySelector('[data-ai-policy] [name=model_id]')
    ?.addEventListener('change', modelPrice);
  root.querySelector('[data-ai-policy]').addEventListener('submit', (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    run(async () => {
      await api(
        '/api/account/ai/settings',
        {
          user_ai_enabled:
            field(form, 'user_ai_enabled')?.checked ?? policy.user_ai_enabled,
          funding: field(form, 'funding')?.value ?? policy.funding,
          model_id: field(form, 'model_id')
            ? field(form, 'model_id').value || null
            : policy.model_id,
          revision: policy.revision,
        },
        'PUT',
      );
      await loadAI();
      notice(
        root.dataset.aiSection === 'setup' ? 'AI setup saved.' : 'AI preferences saved.',
      );
    });
  });
  root.querySelector('[data-ai-request-access]')?.addEventListener('click', () =>
    run(async () => {
      await api('/api/account/ai/access-request', {}, 'POST');
      await loadAI();
      notice('Access request sent. An administrator will review it.');
    }),
  );
  root.querySelector('[data-ai-key]')?.addEventListener('submit', (event) => {
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
      notice('OpenRouter key connected. Your AI feature switch was preserved.');
    });
  });
  root.querySelector('[data-ai-remove-key]')?.addEventListener('click', () =>
    run(async () => {
      await api('/api/account/ai/key', undefined, 'DELETE');
      await loadAI();
      notice('Key removed. Choose a payment source before using AI again.');
    }),
  );
}
function currencyConfig(code) {
  return (
    billing.currencies.find((item) => item.code === code) || {
      code: 'EUR',
      eur_rate_micros: 1000000,
    }
  );
}
function paymentQuote(minor, code) {
  const config = currencyConfig(code);
  const gross = (BigInt(minor) * 10000000000n) / BigInt(config.eur_rate_micros);
  const payment =
    (gross * BigInt(config.payment_fee_bps || 0) + 9999n) / 10000n +
    BigInt(config.payment_fee_fixed_micro || 0);
  const forex = code === 'EUR' ? 0n : (gross + 99n) / 100n;
  return {
    gross: Number(gross),
    payment: Number(payment),
    forex: Number(forex),
    credit: Number(gross - payment - forex),
  };
}
function showAutoConversion() {
  const form = root.querySelector('[data-auto-topup]');
  const preview = root.querySelector('[data-auto-conversion]');
  if (!preview) return;
  try {
    const quote = paymentQuote(cents(field(form, 'amount')), 'EUR');
    preview.textContent = `Each payment adds ${money(Math.max(0, quote.credit))} in credits after ${money(quote.payment)} processing allowance. A change in fee policy pauses auto-top-up until you approve it again.`;
  } catch {
    preview.textContent = 'Enter a payment amount to preview your credits.';
  }
}
function topupQuote() {
  const form = root.querySelector('[data-credit-topup]');
  const code = field(form, 'currency').value;
  return paymentQuote(cents(field(form, 'amount')), code);
}
function showTopupEquivalent() {
  const form = root.querySelector('[data-credit-topup]');
  const code = field(form, 'currency').value;
  const rate = currencyConfig(code).eur_rate_micros;
  const minimum = Math.ceil((500 * rate) / 1000000);
  field(form, 'amount').min = (minimum / 100).toFixed(2);
  field(form, 'amount').max = (Math.floor((100000 * rate) / 1000000) / 100).toFixed(2);
  root.querySelector('[data-topup-equivalent]').textContent =
    `Minimum payment ${money(minimum, code, 100)} (EUR 5 before deductions). Only currencies configured by this instance are available.`;
  root.querySelectorAll('[data-topup-preset]').forEach((button) => {
    button.textContent = money(Number(button.dataset.topupPreset) * 100, code, 100);
    button.disabled =
      Number(button.dataset.topupPreset) * 100 < minimum || !billing.enabled;
  });
  const detail = root.querySelector('[data-topup-conversion]');
  if (!detail) return;
  detail.replaceChildren();
  try {
    const quote = topupQuote();
    metric(detail, 'Converted payment', money(quote.gross));
    metric(detail, 'Payment processing allowance', `−${money(quote.payment)}`);
    metric(detail, 'Currency conversion allowance (1%)', `−${money(quote.forex)}`);
    metric(detail, 'Credits you receive', money(Math.max(0, quote.credit)));
  } catch {
    detail.append(element('p', 'Enter a payment amount to preview your credits.'));
  }
}
function renderBilling() {
  const wallet = root.querySelector('[data-wallet]');
  wallet.replaceChildren();
  metric(wallet, 'Available to spend', money(billing.wallet.available_micro));
  const details = root.querySelector('[data-wallet-details]');
  if (details) {
    details.replaceChildren();
    metric(details, 'Purchased credits', money(billing.wallet.paid_micro));
    metric(details, 'Free credits', money(billing.wallet.free_micro));
    if (billing.wallet.held_micro > 0)
      metric(details, 'AI requests in progress', money(billing.wallet.held_micro));
  }
  const pending = root.querySelector('[data-wallet-pending]');
  if (pending) {
    pending.hidden = !(billing.wallet.held_micro > 0);
    pending.textContent = `${money(billing.wallet.held_micro)} is temporarily set aside for AI requests in progress. Unused credits return automatically when they finish.`;
  }
  const topup = root.querySelector('[data-credit-topup]');
  const currency = field(topup, 'currency');
  const selected = currency.value;
  currency.replaceChildren(
    ...billing.currencies.map((item) =>
      element(
        'option',
        `${item.code} · ${{ EUR: 'Euro', USD: 'US dollar', GBP: 'Pound sterling', CAD: 'Canadian dollar' }[item.code] || item.code}`,
        { value: item.code },
      ),
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
  showAutoConversion();
  const pauseReason =
    {
      fee_policy_changed:
        'Processing fees changed. Review the credits below and enable again to accept them.',
      payment_dispute: 'A payment dispute needs review.',
      payment_failed: 'Your last payment failed.',
      payment_unknown: 'Your last payment is still being verified.',
      ai_unavailable: 'Managed AI is currently unavailable for your account.',
      global_ai_disabled: 'Built-in AI is paused for this instance.',
      billing_review: 'Your billing account needs administrator review.',
      authentication_required: 'Your payment needs authentication.',
      auth_required: 'Your payment needs authentication.',
      byok_enabled: 'Your own OpenRouter key is active.',
      ai_disabled: 'Your personal AI switch is off.',
    }[auto.paused_reason] ||
    (auto.paused_reason ? 'Paused. Review your settings before enabling again.' : '');
  root.querySelector('[data-auto-state]').textContent =
    `${auto.enabled ? 'Enabled' : 'Disabled'}${pauseReason ? ` · ${pauseReason}` : ''}. ${auto.payment_method_configured ? 'Payment method is saved.' : 'Save a payment method during a top-up before enabling this.'}`;
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
  field(topup, 'currency').addEventListener('change', () => {
    const minimum =
      Math.ceil(
        (500 * currencyConfig(field(topup, 'currency').value).eur_rate_micros) / 1000000,
      ) / 100;
    field(topup, 'amount').value = Math.max(
      Number(field(topup, 'amount').value),
      minimum,
    ).toFixed(2);
    showTopupEquivalent();
  });
  field(topup, 'amount').addEventListener('input', showTopupEquivalent);
  root.querySelectorAll('[data-topup-preset]').forEach((button) =>
    button.addEventListener('click', () => {
      field(topup, 'amount').value = Number(button.dataset.topupPreset).toFixed(2);
      showTopupEquivalent();
    }),
  );

  topup.addEventListener('submit', (event) => {
    event.preventDefault();
    run(async () => {
      const payload = {
        amount_minor: cents(field(topup, 'amount')),
        quoted_credit_micro: topupQuote().credit,
        currency: field(topup, 'currency').value,
        save_for_auto_topup: field(topup, 'save_for_auto_topup').checked,
      };
      // A quote/configuration refresh must not turn an uncertain payment retry
      // into a new charge. Its frozen purchase is authoritative on retries.
      const fingerprint = JSON.stringify({
        amount_minor: payload.amount_minor,
        currency: payload.currency,
        save_for_auto_topup: payload.save_for_auto_topup,
      });
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
  field(root.querySelector('[data-auto-topup]'), 'amount').addEventListener(
    'input',
    showAutoConversion,
  );
  root.querySelector('[data-auto-topup]').addEventListener('submit', (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    run(async () => {
      await api(
        '/api/account/billing/auto-topup',
        {
          enabled: field(form, 'enabled').checked,
          amount_minor: cents(field(form, 'amount')),
          quoted_credit_micro: paymentQuote(cents(field(form, 'amount')), 'EUR').credit,
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
  if (type === 'number') {
    input.step = name === 'margin_bps' ? '0.01' : '0.000001';
    input.min = '0';
  }
  node.append(input);
  return node;
}
function renderUsers(users, requests) {
  const queue = root.querySelector('[data-admin-requests]');
  if (queue) {
    queue.replaceChildren();
    for (const item of requests.filter((item) => item.state === 'pending')) {
      const row = element('div', undefined, { class: 'ai-access-request' });
      row.append(
        element(
          'span',
          `${item.name || item.email || item.user_id} · Requested ${timestamp(item.requested_at)}`,
        ),
      );
      const review = element('button', 'Review request', {
        type: 'button',
        class: 'secondary',
      });
      review.addEventListener('click', () => run(() => chooseAdminAccount(item.user_id)));
      row.append(review);
      queue.append(row);
    }
    if (!queue.children.length) queue.append(element('p', 'No pending access requests.'));
  }
  const container = root.querySelector('[data-admin-users]');
  populateAdminAccounts(users);
  if (!container) return;
  container.replaceChildren();
  const selected = users.find((user) => user.user_id === selectedAdminAccount?.user_id);
  if (!selected) {
    container.append(element('p', 'Search for an account above to manage access.'));
    return;
  }
  for (const user of [selected]) {
    const form = element('form', undefined, { class: 'ai-admin-user' });
    form.append(
      element('h3', adminAccountLabel(user)),
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
        'Daily spend limit (€)',
        'daily_limit_micro',
        Number(user.daily_limit_micro || 0) / 1000000,
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
            daily_limit_micro: adminMicro(field(form, 'daily_limit_micro').value),
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
    const restrictionDetails = element('details', undefined, { class: 'ai-advanced' });
    restrictionDetails.open = Boolean(user.billing_blocked || user.debt_micro > 0);
    restrictionDetails.append(
      element('summary', 'Billing restrictions and debt'),
      billingForm,
    );
    container.append(restrictionDetails);
  }
}
let selectedAdminModel = '';
function renderAdminModels(items) {
  const container = root.querySelector('[data-admin-models]');
  if (!container) return;
  container.replaceChildren();
  const create = root.querySelector('[data-admin-model-create]');
  const addOpen = root.querySelector('[data-admin-model-add-open]');
  if (addOpen)
    addOpen.onclick = () => {
      create.hidden = false;
      field(create, 'id').focus();
    };
  const addCancel = root.querySelector('[data-admin-model-add-cancel]');
  if (addCancel)
    addCancel.onclick = () => {
      create.hidden = true;
    };
  if (create)
    create.onsubmit = (event) => {
      event.preventDefault();
      run(async () => {
        selectedAdminModel = field(create, 'id').value.trim();
        await api(
          '/api/admin/ai/models',
          { id: selectedAdminModel, name: field(create, 'name').value.trim() },
          'POST',
        );
        create.reset();
        create.hidden = true;
        await loadAdmin();
        notice('Model added. Review its settings before enabling it.');
      });
    };
  const table = element('table', undefined, { class: 'ai-model-list' });
  const thead = element('thead');
  const labels = element('tr');
  for (const title of ['Model', 'Status', 'Features', ''])
    labels.append(element('th', title));
  thead.append(labels);
  table.append(thead);
  const rows = element('tbody');
  for (const model of items) {
    const row = element('tr');
    const name = element('td');
    name.append(element('strong', model.name || model.id), element('small', model.id));
    row.append(
      name,
      element('td', model.enabled ? 'Enabled' : 'Disabled'),
      element(
        'td',
        [model.classify ? 'Classification' : '', model.draft ? 'Drafting' : '']
          .filter(Boolean)
          .join(' · ') || 'Not configured',
      ),
    );
    const actions = element('td');
    const edit = element('button', selectedAdminModel === model.id ? 'Editing' : 'Edit', {
      type: 'button',
      class: 'secondary',
      'aria-label': `Edit ${model.name || model.id}`,
    });
    edit.addEventListener('click', () => {
      selectedAdminModel = model.id;
      renderAdminModels(items);
      root
        .querySelector('.ai-admin-model')
        ?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    });
    actions.append(edit);
    row.append(actions);
    rows.append(row);
  }
  table.append(rows);
  const scroll = element('div', undefined, { class: 'ai-table-scroll' });
  scroll.append(table);
  container.append(scroll);
  for (const model of items.filter((item) => item.id === selectedAdminModel)) {
    const form = element('form', undefined, { class: 'ai-admin-model' });
    form.append(
      element('h3', model.name || model.id),
      element('p', model.id),
      inputLabel('Display name', 'name', model.name),
      inputLabel('Enabled', 'enabled', model.enabled, 'checkbox'),
      inputLabel('Classification supported', 'classify', model.classify, 'checkbox'),
      inputLabel('Drafting supported', 'draft', model.draft, 'checkbox'),
      inputLabel(
        'Provider input cost (€ / million tokens)',
        'input_micro_per_million',
        Number(model.input_micro_per_million || 0) / 1000000,
        'number',
      ),
      inputLabel(
        'Provider output cost (€ / million tokens)',
        'output_micro_per_million',
        Number(model.output_micro_per_million || 0) / 1000000,
        'number',
      ),
      inputLabel(
        'Provider cached-input cost (€ / million tokens)',
        'cached_micro_per_million',
        Number(model.cached_micro_per_million || 0) / 1000000,
        'number',
      ),
      inputLabel(
        'Gross margin % (blank uses provider default)',
        'margin_bps',
        model.margin_bps === -1 ? '' : Number(model.margin_bps) / 100,
        'number',
      ),
      inputLabel(
        'Daily spend limit (€)',
        'daily_limit_micro',
        Number(model.daily_limit_micro || 0) / 1000000,
        'number',
      ),
      element('button', 'Save model', {
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
          value[name] =
            name === 'margin_bps'
              ? field(form, name).value === ''
                ? -1
                : Math.round(Number(field(form, name).value) * 100)
              : adminMicro(field(form, name).value);
        await api('/api/admin/ai/models', value, 'PUT');
        await loadAdmin();
        notice(
          'Future model pricing updated. Existing quotes keep their accepted rates.',
        );
      });
    });
    const remove = element('button', 'Remove model', {
      type: 'button',
      class: 'secondary danger-text',
    });
    remove.addEventListener('click', () => {
      if (
        !confirm(
          `Remove ${model.name || model.id}? Users will no longer be able to select it. Usage and payment history will be kept.`,
        )
      )
        return;
      run(async () => {
        await api(
          '/api/admin/ai/models',
          { id: model.id, version: model.version },
          'DELETE',
        );
        selectedAdminModel = '';
        await loadAdmin();
        notice('Model removed. Usage history was kept.');
      });
    });
    const close = element('button', 'Close editor', {
      type: 'button',
      class: 'secondary',
    });
    close.addEventListener('click', () => {
      selectedAdminModel = '';
      renderAdminModels(items);
    });
    const actions = element('div', undefined, { class: 'ai-preset-row' });
    actions.append(close, remove);
    form.append(actions);
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
  if (!container) return;
  container.replaceChildren();
  for (const [label, key] of [
    ['Top-ups collected', 'topups_micro'],
    ['Conversion and processing revenue', 'conversion_revenue_micro'],
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
    data.usage_revenue_micro + (data.conversion_revenue_micro || 0) > 0 &&
      data.contribution_micro != null
      ? `${((100 * data.contribution_micro) / (data.usage_revenue_micro + (data.conversion_revenue_micro || 0))).toFixed(2)}%`
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
function adminAccountLabel(user) {
  const name = user.name || user.display_name;
  return [name, user.email, user.user_id].filter(Boolean).join(' · ');
}
function populateAdminAccounts(users) {
  if (selectedAdminAccount) {
    const refreshed = users.find((user) => user.user_id === selectedAdminAccount.user_id);
    if (refreshed) selectedAdminAccount = refreshed;
  }
  for (const picker of root.querySelectorAll('[data-admin-account-picker]')) {
    const selected = picker.querySelector('[data-account-selection]');
    picker.querySelector('[data-admin-account-select]').value =
      selectedAdminAccount?.user_id || '';
    selected.textContent = selectedAdminAccount
      ? `Selected: ${adminAccountLabel(selectedAdminAccount)}`
      : 'No account selected.';
  }
}
function bindAdminAccountPickers() {
  for (const picker of root.querySelectorAll('[data-admin-account-picker]')) {
    const query = picker.querySelector('[data-account-query]');
    const result = picker.querySelector('[data-account-results]');
    const resultsId = `admin-account-results-${++accountPickerSequence}`;
    result.id = resultsId;
    query.setAttribute('aria-controls', resultsId);
    let timer,
      sequence = 0,
      active = -1;
    const close = () => {
      result.hidden = true;
      query.setAttribute('aria-expanded', 'false');
      query.removeAttribute('aria-activedescendant');
      active = -1;
    };
    const choose = async (account) => {
      selectedAdminAccount = account;
      query.value = '';
      close();
      populateAdminAccounts([account]);
      picker.dispatchEvent(new Event('input', { bubbles: true }));
      renderUsers([account], adminAccessRequests);
    };
    const search = async () => {
      const version = ++sequence;
      const text = query.value.trim();
      if (!text) {
        close();
        return;
      }
      result.hidden = false;
      result.replaceChildren(element('p', 'Searching accounts…'));
      query.setAttribute('aria-expanded', 'true');
      try {
        const data = await api(
          `/api/admin/ai/users?q=${encodeURIComponent(text)}&limit=20`,
        );
        if (version !== sequence || query.value.trim() !== text) return;
        result.replaceChildren();
        active = -1;
        for (const account of list(data, 'users')) {
          const button = element('button', adminAccountLabel(account), {
            type: 'button',
            role: 'option',
            id: `${resultsId}-${result.children.length}`,
            'aria-selected': 'false',
            tabindex: '-1',
          });
          button.addEventListener('click', () => {
            choose(account);
            query.focus();
          });
          result.append(button);
        }
        if (!result.children.length)
          result.append(
            element('p', 'No matching accounts. Try a name, email or exact account ID.'),
          );
        else if (result.children.length === 20)
          result.append(
            element(
              'p',
              'Showing the first 20 matches. Type more to narrow your search.',
            ),
          );
      } catch (error) {
        if (version === sequence) result.replaceChildren(element('p', error.message));
      }
    };
    query.addEventListener('input', () => {
      clearTimeout(timer);
      ++sequence;
      timer = setTimeout(search, 250);
    });
    query.addEventListener('keydown', (event) => {
      const options = [...result.querySelectorAll('[role=option]')];
      if (event.key === 'Escape') {
        ++sequence;
        close();
        return;
      }
      if (event.key === 'Enter') event.preventDefault();
      if (
        !['ArrowDown', 'ArrowUp', 'Enter'].includes(event.key) ||
        !options.length ||
        result.hidden
      )
        return;
      event.preventDefault();
      if (event.key === 'Enter') {
        if (active >= 0) options[active].click();
        return;
      }
      active =
        (active + (event.key === 'ArrowDown' ? 1 : -1) + options.length) % options.length;
      options.forEach((option, index) =>
        option.setAttribute('aria-selected', String(index === active)),
      );
      query.setAttribute('aria-activedescendant', options[active].id);
      options[active].scrollIntoView({ block: 'nearest' });
    });
    picker.addEventListener('focusout', (event) => {
      if (!picker.contains(event.relatedTarget)) {
        ++sequence;
        close();
      }
    });
  }
}
async function chooseAdminAccount(userId) {
  const data = await api(
    `/api/admin/ai/users?user_id=${encodeURIComponent(userId)}&limit=1`,
  );
  const user = list(data, 'users')[0];
  if (!user) throw Error('This account is no longer available.');
  selectedAdminAccount = user;
  renderUsers([user], adminAccessRequests);
}
function adminMicro(raw) {
  if (!/^-?\d+(?:\.\d{1,6})?$/.test(raw) || raw.length > 20)
    throw Error('Enter an amount with at most six decimal places.');
  const negative = raw.startsWith('-');
  const [whole, fraction = ''] = raw.replace(/^-/, '').split('.');
  const value = BigInt(whole) * 1000000n + BigInt(fraction.padEnd(6, '0'));
  if (value > BigInt(Number.MAX_SAFE_INTEGER)) throw Error('Amount is too large.');
  return Number(negative ? -value : value);
}
async function loadAdmin() {
  const has = (selector) => Boolean(root.querySelector(selector));
  const fetchIf = (selector, url, fallback) =>
    has(selector) ? api(url) : Promise.resolve(fallback);
  const needsUsers = Boolean(selectedAdminAccount);
  const [users, requests, configured, finance, provider, audit] = await Promise.all([
    needsUsers
      ? api(
          `/api/admin/ai/users?user_id=${encodeURIComponent(selectedAdminAccount.user_id)}&limit=1`,
        )
      : { users: [] },
    fetchIf(
      '[data-admin-requests]',
      `/api/admin/ai/access-requests?limit=50&after=${encodeURIComponent(adminRequestsAfter)}`,
      { requests: [] },
    ),
    fetchIf('[data-admin-models]', '/api/admin/ai/models', { models: [] }),
    fetchIf('[data-admin-finance]', financeURL(), null),
    fetchIf('[data-admin-provider]', '/api/admin/ai/provider', null),
    fetchIf(
      '[data-admin-audit]',
      `/api/admin/ai/audit?limit=25${adminAuditAfter ? `&after=${encodeURIComponent(adminAuditAfter)}` : ''}`,
      { entries: [] },
    ),
  ]);
  adminRequestsNext = requests.next_cursor || '';
  adminAuditNext = audit.next_cursor || '';
  for (const [selector, cursor] of [
    ['[data-admin-audit-next]', adminAuditNext],
    ['[data-admin-requests-next]', adminRequestsNext],
  ]) {
    const button = root.querySelector(selector);
    if (button) button.hidden = !cursor;
  }
  const auditContainer = root.querySelector('[data-admin-audit]');
  if (auditContainer) {
    auditContainer.replaceChildren();
    for (const event of audit.entries || audit.events || audit.audit || [])
      auditContainer.append(
        element(
          'p',
          `${timestamp(event.created_at)} · ${event.action || event.kind} · ${event.target || event.target_user_id || event.user_id || ''}`,
        ),
      );
    if (!auditContainer.children.length)
      auditContainer.append(element('p', 'No audit events yet.'));
  }
  providerConfig = provider;
  const form = root.querySelector('[data-admin-provider]');
  if (form && provider) {
    field(form, 'enabled').checked = provider.enabled;
    field(form, 'margin_bps').value = provider.margin_bps / 100;
    field(form, 'usd_eur_ppm').value = provider.usd_eur_ppm / 1000000;
    field(form, 'daily_limit_micro').value = provider.daily_limit_micro / 1000000;
  }
  adminAccessRequests = list(requests, 'requests');
  renderUsers(list(users, 'users'), adminAccessRequests);
  renderAdminModels(list(configured, 'models'));
  if (finance) renderFinance(finance);
  notice(
    `Up to date · ${new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`,
  );
}
function bindAdmin() {
  bindAdminAccountPickers();
  root
    .querySelector('[data-admin-stripe-reconcile]')
    ?.addEventListener('submit', (event) => {
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
    ?.addEventListener('submit', (event) => {
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
  const adjustmentForm = root.querySelector('[data-admin-adjustment]');
  if (adjustmentForm)
    field(adjustmentForm, 'reference').value ||= `adjustment-${crypto.randomUUID()}`;
  const grantForm = root.querySelector('[data-admin-grant]');
  if (grantForm) {
    let saved = '';
    try {
      saved = sessionStorage.getItem('todoer:admin-grant-reference') || '';
    } catch {}
    field(grantForm, 'reference').value ||= saved || `grant-${crypto.randomUUID()}`;
    grantForm.addEventListener('input', () => {
      const summary = root.querySelector('[data-admin-grant-summary]');
      if (!summary) return;
      const account = field(grantForm, 'user_id');
      summary.textContent =
        account.value && field(grantForm, 'amount').value
          ? `${money(Number(field(grantForm, 'amount').value) * 1000000)} in free AI credits will be added to ${selectedAdminAccount ? adminAccountLabel(selectedAdminAccount) : account.value}. No payment is collected.`
          : 'Choose an account and enter an amount before issuing credits.';
    });
  }
  root.querySelector('[data-admin-grant]')?.addEventListener('submit', (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    run(async () => {
      if (!field(form, 'user_id').value)
        throw Error('Search for and select an account first.');
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
      try {
        sessionStorage.setItem(
          'todoer:admin-grant-reference',
          field(form, 'reference').value,
        );
      } catch {}
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
      field(form, 'reference').value = `grant-${crypto.randomUUID()}`;
      try {
        sessionStorage.removeItem('todoer:admin-grant-reference');
      } catch {}
      await loadAdmin();
      notice('Free credit grant recorded. No payment or revenue was recorded.');
    });
  });
  root.querySelector('[data-admin-audit-next]')?.addEventListener('click', () => {
    adminAuditAfter = adminAuditNext;
    run(loadAdmin);
  });
  root.querySelector('[data-admin-requests-next]')?.addEventListener('click', () => {
    adminRequestsAfter = adminRequestsNext;
    run(loadAdmin);
  });
  root.querySelector('[data-admin-rotate]')?.addEventListener('click', () =>
    run(async () => {
      if (
        !confirm(
          'Re-encrypt a batch of saved BYOK keys using the configured current encryption key? Keep the previous key available until rotation is complete.',
        )
      )
        return;
      const result = await api('/api/admin/ai/keys/rotate', {}, 'POST');
      await loadAdmin();
      notice(
        `Rotated ${result.rotated} keys using the configured encryption keys. Rotation is bounded; repeat for remaining keys if necessary.`,
      );
    }),
  );
  root.querySelector('[data-admin-provider]')?.addEventListener('submit', (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    run(async () => {
      const value = {
        enabled: field(form, 'enabled').checked,
        version: providerConfig.version,
      };
      value.margin_bps = Math.round(Number(field(form, 'margin_bps').value) * 100);
      value.usd_eur_ppm = adminMicro(field(form, 'usd_eur_ppm').value);
      value.daily_limit_micro = adminMicro(field(form, 'daily_limit_micro').value);
      await api('/api/admin/ai/provider', value, 'PUT');
      await loadAdmin();
      notice('Provider controls saved.');
    });
  });
  root.querySelector('[data-admin-adjustment]')?.addEventListener('submit', (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    run(async () => {
      if (!field(form, 'user_id').value)
        throw Error('Search for and select an account first.');
      await api(
        '/api/admin/ai/adjustments',
        {
          user_id: field(form, 'user_id').value,
          amount_micro: adminMicro(field(form, 'amount').value),
          reference: field(form, 'reference').value,
          reason: field(form, 'reason').value,
        },
        'POST',
      );
      await loadAdmin();
      field(form, 'reference').value = `adjustment-${crypto.randomUUID()}`;
      notice('Credit adjustment recorded.');
    });
  });
  root.querySelector('[data-admin-reconcile]')?.addEventListener('click', () =>
    run(async () => {
      if (
        !confirm(
          'Release expired AI reservations that are eligible for reconciliation? This does not retry inference or charge users.',
        )
      )
        return;
      const result = await api('/api/admin/ai/reconcile', {}, 'POST');
      await loadAdmin();
      notice(
        `Reconciliation completed${result.released == null ? '.' : `: ${result.released} expired holds released.`}`,
      );
    }),
  );
}

if (root) {
  // Capture the initiating control before its handler starts a request.
  for (const type of ['click', 'submit'])
    root.addEventListener(
      type,
      (event) => {
        if (busy) {
          event.preventDefault();
          event.stopImmediatePropagation();
          return;
        }
        actionTrigger =
          type === 'submit'
            ? event.submitter ||
              event.target.querySelector('button[type=submit], button:not([type])')
            : event.target.closest('button');
        const current = actionTrigger;
        setTimeout(() => {
          if (actionTrigger === current) actionTrigger = undefined;
        });
      },
      true,
    );
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
