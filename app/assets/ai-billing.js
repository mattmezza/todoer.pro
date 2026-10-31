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
  node.hidden = !message;
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
  const loadingLabel = trigger?.matches('[data-ai-reload]')
    ? 'Refreshing…'
    : trigger?.type === 'submit'
      ? 'Saving…'
      : 'Working…';
  if (trigger) {
    trigger.textContent = loadingLabel;
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
      if (trigger.textContent === loadingLabel) trigger.replaceChildren(...priorContent);
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
let aiCreditState;
function aiFundingReady(funding = policy.funding) {
  if (funding === 'credits') return policy.managed_ai_allowed && !policy.billing_blocked;
  if (funding === 'byok')
    return policy.byok_allowed && policy.key_configured && policy.key_status === 'valid';
  return false;
}
function aiModelReady(id = policy.model_id) {
  return models.some((model) => model.enabled && model.id === id);
}
function enableAIIntent() {
  return new URLSearchParams(location.search).get('enable') === '1';
}
function renderPolicy() {
  const form = root.querySelector('[data-ai-policy]');
  const enabled = field(form, 'user_ai_enabled');
  if (enabled) {
    enabled.checked = policy.user_ai_enabled;
    const ready =
      aiFundingReady() &&
      aiModelReady() &&
      (policy.funding !== 'credits' || (aiCreditState?.wallet?.available_micro ?? 0) > 0);
    form.hidden = !policy.user_ai_enabled && !ready;
    const firstUse = root.querySelector('[data-ai-first-use]');
    if (firstUse) firstUse.hidden = !form.hidden;
    const availability = root.querySelector('[data-ai-availability]');
    if (availability)
      availability.textContent = models.length
        ? policy.funding === 'credits' && aiFundingReady() && aiModelReady()
          ? aiCreditState
            ? 'Add credits before enabling AI. Setup will show your next step.'
            : 'Review your credit balance in setup before enabling AI.'
          : 'Choose a model and payment source first. You will review your setup before enabling AI.'
        : 'Built-in AI is not ready on this instance yet. Setup explains what is needed before you can enable it.';
  }
  const funding = field(form, 'funding');
  if (funding) {
    funding.value = policy.funding === 'none' ? '' : policy.funding;
    form.querySelector('[name=funding][value=byok]').disabled = !policy.byok_allowed;
  }
  const entitlement = root.querySelector('[data-ai-entitlement]');
  if (entitlement)
    entitlement.textContent = policy.billing_blocked
      ? 'Billing needs administrator review before you can use Todoer credits.'
      : policy.managed_ai_allowed
        ? 'Your access is approved. Credits are only used when you request AI help; add credits in Billing when needed.'
        : policy.access_request === 'pending'
          ? 'Your request is being reviewed. You can return later or use your own OpenRouter key instead.'
          : 'Request access before using Todoer credits. An administrator will review your request.';
  const creditAction = root.querySelector('[data-ai-credit-action]');
  if (creditAction)
    creditAction.hidden = !policy.managed_ai_allowed || policy.billing_blocked;
  const requestAccess = root.querySelector('[data-ai-request-access]');
  if (requestAccess) {
    requestAccess.hidden = policy.managed_ai_allowed;
    requestAccess.disabled = policy.access_request === 'pending';
    requestAccess.textContent =
      policy.access_request === 'pending' ? 'Request pending' : 'Request access';
  }
  const keyStatus = root.querySelector('[data-ai-key-status]');
  if (keyStatus)
    keyStatus.textContent = !policy.byok_allowed
      ? 'Using your own key is disabled by an administrator.'
      : policy.key_configured && policy.key_status === 'valid'
        ? 'Your OpenRouter key is connected. Finish setup below when you are ready.'
        : policy.key_configured
          ? 'Your saved key needs to be reconnected before you can use it.'
          : 'Connect your OpenRouter key after choosing a model above.';
  const removeKey = root.querySelector('[data-ai-remove-key]');
  if (removeKey) removeKey.hidden = !policy.key_configured;
  modelOptions();
  renderAISetupState();
}
function modelPrice() {
  const select = root.querySelector('[data-ai-policy] [name=model_id]');
  const price = root.querySelector('[data-ai-model-price]');
  if (!select || !price) return;
  const model = models.find((item) => item.id === select.value);
  const funding = field(root.querySelector('[data-ai-policy]'), 'funding').value;
  price.textContent = model
    ? funding === 'byok'
      ? `OpenRouter · ${model.name || model.id}. Your OpenRouter account pays the provider's applicable rates.`
      : `OpenRouter · ${model.name || model.id}. Credit rates per million tokens: input ${money(model.input_micro_per_million)}, output ${money(model.output_micro_per_million)}.`
    : 'Choose a supported model to see its rates.';
}
function renderAISetupState() {
  if (root.dataset.aiSection !== 'setup') return;
  const form = root.querySelector('[data-ai-policy]');
  const funding = field(form, 'funding').value;
  const selectedModel = field(form, 'model_id').value;
  const modelReady = aiModelReady(selectedModel);
  const available = models.some((model) => model.enabled);
  const guided = enableAIIntent();
  const availability = root.querySelector('[data-ai-model-availability]');
  availability.hidden = available;
  availability.textContent =
    'No supported models are available on this instance yet. Ask an administrator to enable a model, then return to setup.';
  field(form, 'model_id').disabled = !available;
  root.querySelector('[data-ai-credits-panel]').hidden = funding !== 'credits';
  root.querySelector('[data-ai-key-panel]').hidden = funding !== 'byok';
  const keySubmit = root.querySelector('[data-ai-key] button[type=submit]');
  keySubmit.disabled = !policy.byok_allowed || !modelReady;
  root.querySelector('[data-ai-enable-disclosure]').hidden =
    !guided || policy.user_ai_enabled;
  const creditReady = (aiCreditState?.wallet?.available_micro ?? 0) > 0;
  const ready =
    modelReady &&
    aiFundingReady(funding) &&
    (!guided || funding !== 'credits' || creditReady);
  const action = root.querySelector('[data-ai-setup-submit]');
  action.disabled = !ready;
  action.textContent = guided && !policy.user_ai_enabled ? 'Enable AI' : 'Save AI setup';
  root.querySelector('[data-ai-setup-heading]').textContent = guided
    ? 'Set up and enable AI'
    : 'AI setup';
  const next = root.querySelector('[data-ai-setup-next]');
  next.textContent = !available
    ? 'AI cannot be enabled until an administrator makes a supported model available.'
    : !funding
      ? 'Choose Todoer credits or your own OpenRouter key to continue.'
      : !modelReady
        ? 'Choose a supported model above to continue.'
        : funding === 'credits' && policy.billing_blocked
          ? 'An administrator needs to review your billing account before you can continue.'
          : funding === 'credits' && !policy.managed_ai_allowed
            ? policy.access_request === 'pending'
              ? 'Your access request is pending. Return after approval, or choose your own OpenRouter key.'
              : 'Request access above. You can finish enabling AI after your request is approved.'
            : funding === 'credits' && !creditReady
              ? !aiCreditState
                ? 'Your credit balance could not be checked. Open Billing to review it before enabling AI.'
                : aiCreditState.enabled
                  ? 'Add credits in Billing before enabling AI. Unused credits stay in your wallet.'
                  : 'Your wallet is empty and credit purchases are not available on this instance yet. Ask an administrator for access or free credits.'
              : funding === 'byok' && !aiFundingReady('byok')
                ? 'Connect your OpenRouter key above to continue. Connecting the key will not enable AI.'
                : guided && !policy.user_ai_enabled
                  ? 'Your model and payment source are ready. Enable AI when you want to start using built-in features.'
                  : 'Save these choices. Your current AI feature switch will stay unchanged.';
  modelPrice();
}
async function loadAI() {
  const setup = root.dataset.aiSection === 'setup';
  let notices;
  [policy, { models }, notices] = await Promise.all([
    api('/api/account/ai/settings'),
    api('/api/account/ai/models'),
    setup ? api('/api/account/ai/notices') : { notices: [] },
  ]);
  aiCreditState = null;
  if (setup || policy.funding === 'credits') {
    try {
      aiCreditState = await api('/api/account/billing');
    } catch {
      /* Show an explicit unavailable balance, never assume zero or ready. */
    }
  }
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
    root.querySelector('[data-ai-updates-panel]').hidden = !notices.notices?.some(
      (item) => item.unread,
    );
  }
  const readUpdates = root.querySelector('[data-ai-notices-read]');
  if (readUpdates) readUpdates.disabled = !notices.notices?.some((item) => item.unread);
  renderPolicy();
  notice('');
}
function bindAI() {
  root.querySelector('[data-ai-notices-read]')?.addEventListener('click', () =>
    run(async () => {
      await api('/api/account/ai/notices', { through: noticeThrough }, 'POST');
      await loadAI();
      notice('Updates marked as read.');
    }),
  );
  const policyForm = root.querySelector('[data-ai-policy]');
  policyForm.addEventListener('change', renderAISetupState);
  policyForm.addEventListener('submit', (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    run(async () => {
      const setup = root.dataset.aiSection === 'setup';
      const funding = field(form, 'funding')?.value ?? policy.funding;
      const model = field(form, 'model_id')
        ? field(form, 'model_id').value || null
        : policy.model_id;
      const enable = setup
        ? enableAIIntent() || policy.user_ai_enabled
        : field(form, 'user_ai_enabled').checked;
      if (enable && (!aiFundingReady(funding) || !aiModelReady(model))) {
        if (setup) renderAISetupState();
        else location.assign('/settings/ai/setup?enable=1');
        return;
      }
      await api(
        '/api/account/ai/settings',
        {
          user_ai_enabled: enable,
          funding,
          model_id: model,
          revision: policy.revision,
        },
        'PUT',
      );
      if (setup && enableAIIntent()) {
        const url = new URL(location.href);
        url.searchParams.delete('enable');
        history.replaceState(null, '', url.pathname + url.search + url.hash);
      }
      await loadAI();
      notice(
        setup && enable
          ? 'AI is enabled. You can turn it off in AI preferences at any time.'
          : setup
            ? 'AI setup saved.'
            : 'AI preferences saved.',
      );
    });
  });
  root.querySelector('[data-ai-request-access]')?.addEventListener('click', () =>
    run(async () => {
      const funding = field(policyForm, 'funding').value;
      const model = field(policyForm, 'model_id').value;
      await api('/api/account/ai/access-request', {}, 'POST');
      await loadAI();
      field(policyForm, 'funding').value = funding;
      field(policyForm, 'model_id').value = model;
      renderAISetupState();
      notice(
        'Access request sent. You can finish setup after an administrator approves it.',
      );
    }),
  );
  root.querySelector('[data-ai-key]')?.addEventListener('submit', (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    if (busy || !policy) return;
    const key = field(form, 'key').value;
    field(form, 'key').value = '';
    run(async () => {
      const selectedModel = field(policyForm, 'model_id').value;
      if (!aiModelReady(selectedModel) || !policy.byok_allowed) return;
      await api(
        '/api/account/ai/key',
        { key, model_id: selectedModel, revision: policy.revision },
        'PUT',
      );
      await loadAI();
      notice(
        'OpenRouter key connected. Your AI feature switch was preserved. Finish setup when you are ready.',
      );
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
function renderWallet() {
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
}
function renderBilling() {
  renderWallet();
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
const returnedPurchase = new URLSearchParams(location.search).get('purchase');
let purchaseReturnFinished = false;
let purchaseReturnTimer;
let purchaseReturnChecks = 0;
function checkPurchaseReturn() {
  if (!returnedPurchase || purchaseReturnFinished) return;
  let banner = root.querySelector('[data-purchase-return]');
  if (!banner) {
    banner = element('div', undefined, {
      'data-purchase-return': '',
      role: 'status',
      class: 'purchase-return',
    });
    root.prepend(banner);
  }
  const purchase = billing.purchases.find((item) => item.id === returnedPurchase);
  if (purchase?.status === 'paid') {
    const message = `Purchase successful. ${money(purchase.credit_micro)} in credits has been added to your account.`;
    banner.textContent = message;
    banner.classList.add('is-success');
    toast(message);
    purchaseReturnFinished = true;
    clearTimeout(purchaseReturnTimer);
    const url = new URL(location.href);
    url.searchParams.delete('purchase');
    history.replaceState(history.state, '', url.pathname + url.search + url.hash);
    purchaseAttempt = undefined;
    try {
      sessionStorage.removeItem(purchaseStorageKey);
    } catch {
      /* Storage may be disabled. */
    }
    return;
  }
  if (
    purchase &&
    ['failed', 'refunded', 'disputed', 'requires_action'].includes(purchase.status)
  ) {
    banner.textContent =
      'This purchase is not confirmed. Check its status in your top-up history below before trying again.';
    purchaseReturnFinished = true;
    return;
  }
  banner.textContent =
    'Welcome back. We’re confirming your payment and updating your credits…';
  if (purchaseReturnTimer) return;
  if (purchaseReturnChecks >= 12) {
    banner.textContent =
      'Your payment is still being confirmed. You don’t need to pay again. Refresh billing shortly to check your credits.';
    return;
  }
  purchaseReturnTimer = setTimeout(async () => {
    purchaseReturnTimer = undefined;
    purchaseReturnChecks++;
    try {
      billing = await api('/api/account/billing');
      renderWallet();
      checkPurchaseReturn();
    } catch {
      banner.textContent =
        'We couldn’t check your payment yet. Refresh billing to check again. You don’t need to pay again.';
    }
  }, 5000);
}
async function loadBilling() {
  billing = await api('/api/account/billing');
  billing.currencies ||= [{ code: 'EUR', eur_rate_micros: 1000000 }];
  renderBilling();
  checkPurchaseReturn();
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
    const spendMetrics = element('div');
    spend.addEventListener('click', () =>
      run(async () => {
        const report = await api(financeURL(user.user_id));
        renderFinanceSummary(spendMetrics, report);
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
      selectedAdminModel = '';
      renderAdminModels(items);
      create.hidden = false;
      container.hidden = true;
      addOpen.hidden = true;
      field(create, 'id').focus();
    };
  const addCancel = root.querySelector('[data-admin-model-add-cancel]');
  if (addCancel)
    addCancel.onclick = () => {
      create.hidden = true;
      container.hidden = false;
      addOpen.hidden = false;
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
  container.hidden = false;
  if (addOpen) addOpen.hidden = Boolean(selectedAdminModel);
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
    scroll.hidden = true;
    const heading = form.querySelector('h3');
    heading.textContent = 'Edit model';
    form.querySelector('p').className = 'ai-model-id';
    const group = (title, names, help) => {
      const section = element('fieldset', undefined, { class: 'ai-model-group' });
      section.append(element('legend', title));
      if (help) section.append(element('p', help, { class: 'ai-model-help' }));
      const grid = element('div', undefined, { class: 'ai-model-fields' });
      for (const name of names) {
        const input = field(form, name);
        const label = input.closest('label');
        if (input.type === 'checkbox') {
          label.className = 'ai-model-check';
          label.prepend(input);
        }
        grid.append(label);
      }
      section.append(grid);
      form.insertBefore(section, actions);
    };
    group('Availability', ['name', 'enabled', 'classify', 'draft']);
    group(
      'Provider costs',
      ['input_micro_per_million', 'output_micro_per_million', 'cached_micro_per_million'],
      'EUR per 1 million tokens. Enter the provider’s costs before enabling this model.',
    );
    group('Pricing & limits', ['margin_bps', 'daily_limit_micro']);
    const shortLabels = {
      input_micro_per_million: 'Input',
      output_micro_per_million: 'Output',
      cached_micro_per_million: 'Cached input',
      margin_bps: 'Gross margin (%)',
      daily_limit_micro: 'Daily spend limit (€)',
      classify: 'Classification',
      draft: 'Drafting',
    };
    for (const [name, text] of Object.entries(shortLabels)) {
      const input = field(form, name);
      const label = input.closest('label');
      for (const node of [...label.childNodes])
        if (node.nodeType === Node.TEXT_NODE) node.textContent = text;
    }
    field(form, 'margin_bps').placeholder = 'Provider default';
    field(form, 'margin_bps').max = '95';
    field(form, 'name').required = true;
    const save = form.querySelector('button[type="submit"]');
    save.className = 'primary';
    save.textContent = 'Save changes';
    close.textContent = 'Cancel';
    actions.className = 'ai-model-actions';
    actions.replaceChildren(remove, close, save);
    container.append(form);
    field(form, 'name').focus();
  }
  if (!items.length) container.append(element('p', 'No models configured.'));
}
function financeURL(userId) {
  const params = new URLSearchParams(financeRange);
  if (userId) params.set('user_id', userId);
  const query = params.toString();
  return `/api/admin/ai/finance${query ? `?${query}` : ''}`;
}
function renderFinanceSummary(container, data) {
  container.replaceChildren();
  const headlines = element('dl', undefined, {
    class: 'ai-metrics ai-finance-headlines',
  });
  const headline = (label, value, help) => {
    metric(headlines, label, value);
    headlines.lastElementChild.lastElementChild.append(element('small', help));
  };
  headline(
    'Money received',
    money(data.net_cash_collected_micro ?? data.topups_micro),
    'Top-ups, after refunds. Some of this is still in user wallets.',
  );
  const costsKnown =
    data.provider_cost_micro != null &&
    data.payment_fees_micro != null &&
    !data.provider_fx_unknown;
  headline(
    'Costs',
    costsKnown
      ? money(data.provider_cost_micro + data.payment_fees_micro)
      : 'Still being confirmed',
    'AI provider costs, including free-credit usage, plus Stripe fees.',
  );
  headline(
    'Profit',
    data.profit_micro == null ? 'Still being confirmed' : money(data.profit_micro),
    'Earned from credits already spent and top-up fees, minus costs and unpaid debt.',
  );
  headline(
    'Unused paid credits',
    money(data.paid_outstanding_micro),
    'Money users can still spend. This is not profit yet.',
  );
  container.append(headlines);
  if (data.profit_micro == null || data.provisional)
    container.append(
      element(
        'p',
        'Some costs are still being confirmed. Profit will appear once the missing costs are recorded.',
        { class: 'muted' },
      ),
    );
  const breakdown = (title, help, entries) => {
    const section = element('details', undefined, { class: 'ai-finance-details' });
    section.append(element('summary', title), element('p', help, { class: 'muted' }));
    const values = element('dl', undefined, { class: 'ai-finance-rows' });
    for (const [label, key] of entries) metric(values, label, money(data[key]));
    section.append(values);
    container.append(section);
  };
  breakdown(
    'Free credit promotions',
    'Grants are not sales. Their AI costs are included in Costs above.',
    [
      ['Free credits issued', 'free_credits_issued_micro'],
      ['Free credits used', 'free_credits_used_micro'],
      ['Free credits remaining', 'free_credits_remaining_micro'],
      ['AI cost of free credits', 'free_inference_cost_micro'],
    ],
  );
  breakdown(
    'Payments and balances',
    'Payments belong to the selected period. Unused balances show what is in wallets right now.',
    [
      ['Top-ups collected', 'topups_micro'],
      ['Refunds', 'refunds_micro'],
      ['Money received after refunds', 'net_cash_collected_micro'],
      ['Unused purchased credits', 'paid_outstanding_micro'],
    ],
  );
  breakdown(
    'How profit is calculated',
    'Profit counts purchased credits when users spend them, plus fees retained from top-ups. AI costs, Stripe fees and unpaid debt are then subtracted.',
    [
      ['Earned from AI usage', 'usage_revenue_micro'],
      ['Fees retained from top-ups', 'conversion_revenue_micro'],
      ['AI provider costs', 'provider_cost_micro'],
      ['Stripe fees', 'payment_fees_micro'],
      ['Unpaid debt', 'bad_debt_micro'],
      ['Profit', 'profit_micro'],
    ],
  );
  metric(
    container.lastElementChild.querySelector('dl'),
    'Profit margin',
    data.margin_bps == null ? 'Not available' : `${(data.margin_bps / 100).toFixed(2)}%`,
  );
  if (data.byok_calls > 0)
    container.append(
      element(
        'p',
        `${data.byok_calls} requests used users’ own OpenRouter keys. Users paid for these directly; they are not included in your costs.`,
        { class: 'muted' },
      ),
    );
}
function renderFinance(data) {
  const container = root.querySelector('[data-admin-finance]');
  if (!container) return;
  renderFinanceSummary(container, data);
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
