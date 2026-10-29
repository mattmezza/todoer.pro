# AI, BYOK and credits

Todoer supports optional built-in AI through OpenRouter. Core task management and external-agent access through MCP, CLI and REST do not consume Todoer AI credits. The source includes BYOK, prepaid credit accounting, Stripe top-ups and administration; availability on a running instance depends on operator configuration. Checkout is disabled until explicitly enabled.

## Choose whether to use AI

Open **Settings → AI**. The personal **Allow built-in AI** switch starts off. Turning it off blocks new built-in inference, including BYOK, and pauses new automatic top-ups. Requests already sent to a provider can finish. Your external MCP, CLI and REST connections remain independently manageable under connected tools.

Managed AI credits require administrator approval. Use **Request access** and follow the decision in account AI updates. Approval does not turn your personal AI switch on. BYOK availability starts on, but an administrator can disable it for an account.

## Bring your own OpenRouter key

1. Choose an enabled supported model in AI settings.
2. Enter your OpenRouter inference API key and select **Validate and save key**. Validation checks the fixed OpenRouter key endpoint without requesting inference.
3. Successful setup selects BYOK funding and pauses auto-top-up, while preserving your AI switch.
4. Enable built-in AI when you are ready to send selected content to OpenRouter.

Keys are write-only, encrypted on the server and bound to your account. Removing or invalidating a key blocks subsequent BYOK calls; Todoer never silently falls back to purchased credits. Only the supported model list is available. Arbitrary provider endpoints and model IDs are not accepted.

BYOK has no Todoer AI usage fee. OpenRouter bills the account associated with your key, including any provider charges incurred by a response that Todoer cannot use. This is your OpenRouter API key, rather than a direct inference-provider key.

## Understand a transaction

Before dispatch, Todoer shows the selected model, OpenRouter routing, funding source and the maximum Todoer credit charge. A managed request requires acceptance of its quote; accepted quotes retain their rates. No automatic inference retries or model/funding fallback are performed.

**Settings → AI usage** shows each operation's feature, funding, model, serving provider when known, input/output tokens, available cache/reasoning details, status and your charge. Managed usage displays the retail amount billed to your wallet. BYOK displays the reported OpenRouter charge in USD and no Todoer credit debit. Unknown metadata is labelled unavailable, not zero. Billing history excludes prompts and task content.

A quote is not an invoice. Managed calls reserve a bounded maximum, then settle a usable result and release unused credit. Failures and interrupted dispatches need release or reconciliation. Core capture continues to work when funds, access or a provider are unavailable. Choosing not to apply a usable draft does not undo its inference transaction.

## Credits and payments

**Settings → Billing** separates posted balance, reservations, available balance, paid credits and free promotional credits. Managed usage records total credit consumption, the portion covered by free credits and the paid portion separately. The internal wallet uses integer micro-EUR; Stripe payment amounts use the currency's minor units. Top-ups accept a custom amount within the instance's bounds, with suggested amounts shown in the UI. EUR checkout is available when billing is configured; USD, GBP and CAD appear only when the operator enables their conversion rates. Each purchase locks its amount, currency and conversion basis.

Stripe handles card entry. Returning from Checkout does not grant credit: only a verified successful payment does. Pending purchases, refunds and adjustments remain distinct from AI usage. Todoer stores payment references and state, not card numbers.

Auto-top-up is optional. Choose a recharge amount, threshold, monthly cap and minimum interval, then explicitly authorize future off-session payments. A saved payment method is required. There is at most one automatic payment in flight per account. It pauses when AI is turned off, BYOK becomes active, managed access is removed, a payment needs authentication, or a spending safeguard prevents another charge. Resuming requires an explicit action. Disabling automatic payments does not recall one already dispatched.

If an automatic payment requires authentication, follow the billing screen's instructions to complete an on-session top-up and re-enable automatic payments. Do not keep submitting new payments to recover an uncertain result; reload saved billing state first.

## Operator configuration

Keep configuration in ignored environment files or secret storage. Never use production data or live payment/provider calls for development checks.

| Variable | Purpose |
|---|---|
| `TODOER_AI_ENABLED` | Set to `0` for an instance-wide inference stop; `1` allows requests that pass account and model controls. |
| `TODOER_OPENROUTER_API_KEY` | Instance key for managed inference. BYOK uses the requesting user's encrypted key. |
| `TODOER_BYOK_ENCRYPTION_KEY` | Dedicated stable encryption material for account OpenRouter keys. Generate and back it up separately. Falls back to the server key if absent. |
| `TODOER_BYOK_PREVIOUS_ENCRYPTION_KEY` | Previous encryption material during a controlled rotation. |
| `TODOER_ASSIST_PROMPT_FILE` | Optional private runtime JSON file with nonempty `classify` and `draft` string fields. It must be readable by the app user. |
| `TODOER_BILLING_ENABLED` | New purchases require `1`; default is disabled. |
| `TODOER_STRIPE_SECRET_KEY` | Stripe server API key; use test mode for integration verification. |
| `TODOER_STRIPE_WEBHOOK_SECRET` | Signing secret for the configured webhook endpoint. |
| `TODOER_BILLING_CURRENCY_RATES` | Optional JSON mapping of lowercase `usd`, `gbp`, `cad` to currency units per EUR scaled by one million. EUR remains fixed. Only configured currencies are offered. |

Use a dedicated random BYOK encryption key rather than changing the server key to rotate BYOK. During rotation, retain the previous key securely, configure both materials, restart the app, and use the admin rotation action in bounded batches until no old records remain. Preserve recovery materials for older backups. Removing the old key too early makes those ciphertexts unreadable.

Private prompt files belong outside Git and image build contexts. Mount the file read-only at runtime and set `TODOER_ASSIST_PROMPT_FILE` to its container path. No prompt contents are included in these instructions. Validation and authorization remain enforced by code regardless of prompt configuration.

## Model readiness and administration

The seeded model entries start disabled. They are configuration candidates, not a claim that a live model was evaluated. Before enabling one, verify availability, feature/schema compatibility, privacy settings, quality and the current provider rates. Configure the provider currency conversion and rate card before enabling managed credit usage. The model list is deliberately bounded; model or provider changes require an explicit operator decision.

A platform administrator sees **Admin** in the account menu. Workspace ownership and an agent credential's admin permission do not grant platform administration. Admin routes require browser authentication; sensitive changes require recent sign-in and CSRF protection.

The panel includes a dedicated **Grant free credits** action requiring an account, positive amount, unique reference and reason. Reusing the same reference and exact details is idempotent. Grants are promotional balance, not a payment or revenue. Positive manual credit adjustments are also recorded as promotional grants. The panel manages access requests, account eligibility and spend caps, provider/model availability, prospective pricing, user balances, usage economics, audited credit adjustments and reconciliation. Model overrides can inherit the provider's configured gross-margin rule. Public user responses contain retail rates, not internal provider expenses or margins.

For a lost Stripe response, the admin recovery form can verify and reconcile an existing PaymentIntent against its local purchase using explicit identifiers and an audit reason; it never creates another charge.

Per-account billing controls let administrators block or restore billing after reviewing payment problems. Forgiving unpaid debt is a separate, explicit write-off action requiring a reason and acknowledgement; it does not refund a payment or erase usage history.

Finance reports separate collected top-ups, net cash collected, paid-credit usage revenue, paid outstanding balances, bad debt, provider expense, payment fees and net profit/margin. Free credits issued, used and remaining, plus their inference expense, are shown separately overall and by model/provider. Promotional grants never count as top-ups or paid revenue. Net profit is unavailable when required provider or payment-fee data is unknown; provisional contribution before fees remains visible. Date ranges filter activity while balances remain current snapshots. BYOK provider charges are excluded from Todoer's managed revenue and expense. Do not treat the operational dashboard as a tax or audited financial statement.

The server starts a bounded billing worker when Stripe reconciliation is configured. Disabling new purchases does not discard signed webhooks or pending reconciliation while credentials remain configured. Watch pending purchases, unknown inference operations and reserved balances; use the audited reconciliation controls rather than editing ledger rows. See [backup and restore](local-recovery.md) before schema upgrades or encryption changes.

## HTTP surfaces

Account settings use browser sessions and CSRF-protected writes; scoped REST/MCP credentials cannot administer keys or payments.

- `/api/account/ai/settings`, `/api/account/ai/key`, `/api/account/ai/access-request`, `/api/account/ai/notices`: account AI controls.
- `/api/account/ai/models`, `/api/account/ai/usage`: supported retail model information and the account's content-free usage history.
- `/api/account/billing`, `/api/account/billing/topups`, `/api/account/billing/auto-topup`: billing state and user-authorized purchases.
- `/api/admin/ai/*`: platform-admin operations; hiding the menu is not the authorization boundary.
- `POST /api/billing/stripe/webhook`: signed raw-body Stripe events, independent of browser login.

Configure Stripe to deliver relevant Checkout completion, successful payment, refund and dispute events to the HTTPS webhook endpoint. Signature verification and purchase binding are required; neither browser-return parameters nor arbitrary event metadata can assign credit to an account. Duplicate/out-of-order processing is idempotent. See [Stripe webhooks](https://docs.stripe.com/webhooks) and [saving payment methods](https://docs.stripe.com/payments/save-during-payment) for the upstream setup.

## Verification

Synthetic provider, ledger, policy and Stripe transport/webhook tests cover money and authorization boundaries without real charges. Browser checks cover key clearing, retained top-up idempotency, safe usage rendering and revision-bound admin forms. They do not establish live model quality, a provider's retention promises, or production checkout readiness. Run `make check`, `make test` and the synthetic billing browser suite before enabling an instance.
