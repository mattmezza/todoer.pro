# AI, BYOK and credits

Todoer supports optional built-in AI through OpenRouter. Core task management and external-agent access through MCP, CLI and REST do not consume Todoer AI credits. The source includes BYOK, prepaid credit accounting, Stripe top-ups and administration; availability on a running instance depends on operator configuration. Checkout is disabled until explicitly enabled.

## Choose whether to use AI

Open **Settings → AI** and choose **Set up and enable AI** for first use. Setup guides you through a payment source, an available model and any access/key requirements, then asks you to explicitly **Enable AI**. Credit-funded AI needs available credits before this final step. No AI is enabled by merely connecting a key or saving ordinary setup changes.

The personal **Allow built-in AI** switch starts off. Once configured, it appears in AI preferences; turning it off blocks new built-in inference, including BYOK, and pauses new automatic top-ups. Requests already sent to a provider can finish. Your external MCP, CLI and REST connections remain independently manageable under **Settings → Connected tools** (`/settings/ai/tools`).

Managed AI credits require administrator approval. Use **Request access** and follow the decision in account AI updates. Approval does not turn your personal AI switch on. BYOK availability starts on, but an administrator can disable it for an account.

AI preferences (`/settings/ai`) control how features behave and whether built-in AI is allowed. AI setup (`/settings/ai/setup`) holds model selection, payment source, access requests and your OpenRouter key. Billing and transaction history have their own screens. Saving changes to an existing setup preserves your AI feature switch. The first-use flow turns it on only when you explicitly choose **Enable AI**. Saving preferences preserves your selected model and payment source.

## Bring your own OpenRouter key

1. Open **Settings → AI setup** (`/settings/ai/setup`) and choose an enabled supported model.
2. Enter your OpenRouter inference API key and select **Validate and save key**. Validation checks the fixed OpenRouter key endpoint without requesting inference.
3. Successful setup selects BYOK funding and pauses auto-top-up, while preserving your AI switch.
4. In the first-use setup flow, select **Enable AI** when you are ready to send selected content to OpenRouter. The action stays unavailable until the selected model and key are ready.

Keys are write-only, encrypted on the server and bound to your account. Removing or invalidating a key blocks subsequent BYOK calls; Todoer never silently falls back to purchased credits. Only the supported model list is available. Arbitrary provider endpoints and model IDs are not accepted.

BYOK has no Todoer AI usage fee. OpenRouter bills the account associated with your key, including any provider charges incurred by a response that Todoer cannot use. This is your OpenRouter API key, rather than a direct inference-provider key.

## Understand a transaction

Before dispatch, Todoer shows the selected model, OpenRouter routing, funding source and the maximum Todoer credit charge. A managed request requires acceptance of its quote; accepted quotes retain their rates. No automatic inference retries or model/funding fallback are performed.

**Settings → AI usage** shows each operation's feature, funding, model, serving provider when known, input/output tokens, available cache/reasoning details, status and your charge. Managed usage displays the retail amount billed to your wallet. BYOK displays the reported OpenRouter charge in USD and no Todoer credit debit. Unknown metadata is labelled unavailable, not zero. Billing history excludes prompts and task content.

A quote is not an invoice. Managed calls reserve a bounded maximum, then settle a usable result and release unused credit. Failures and interrupted dispatches need release or reconciliation. Core capture continues to work when funds, access or a provider are unavailable. Choosing not to apply a usable draft does not undo its inference transaction.

## Credits and payments

**Settings → Billing** separates posted balance, reservations, available balance, paid credits and free promotional credits. Managed usage records total credit consumption, the portion covered by free credits and the paid portion separately. The internal wallet uses integer micro-EUR; Stripe payment amounts use the currency's minor units. Top-ups accept a custom amount within the instance's bounds, with suggested amounts shown in the UI. EUR checkout is available when billing is configured; USD, GBP and CAD appear only when the operator enables their conversion rates. Each purchase locks its amount, currency, conversion basis and resulting credit amount. Credits equal the converted EUR payment minus the instance's published processing allowance (configured percentage plus fixed EUR amount), and minus a 1% conversion allowance for non-EUR payments. The processing allowance is not the card-specific fee eventually reported by Stripe: actual payment fees remain an operator expense and never cause a second wallet debit. For example, a USD 10 payment at USD 1.25 per EUR becomes EUR 8 before deductions; with a 2% plus EUR 0.20 processing allowance and 1% foreign-currency allowance it grants EUR 7.56 credits. No external current exchange rates or Stripe tariffs are assumed.

Stripe handles card entry. Returning from Checkout does not grant credit: only a verified successful payment does. Pending purchases, refunds and adjustments remain distinct from AI usage. Todoer stores payment references and state, not card numbers.

Auto-top-up is optional. Choose a EUR payment amount, threshold, monthly payment cap and minimum interval. Review the net-credit preview after the processing allowance, then explicitly authorize future off-session payments. A saved payment method is required. There is at most one automatic payment in flight per account. It pauses when AI is turned off, BYOK becomes active, managed access is removed, a payment needs authentication, or a spending safeguard prevents another charge. A change to the consented processing allowance also pauses auto-top-up until fresh approval. Resuming requires an explicit action. Disabling automatic payments does not recall one already dispatched.

If an automatic payment requires authentication, follow the billing screen's instructions to complete an on-session top-up and re-enable automatic payments. Do not keep submitting new payments to recover an uncertain result; reload saved billing state first.

## Operator configuration

Keep configuration in ignored environment files or secret storage. Never use production data or live payment/provider calls for development checks.

| Variable                                     | Purpose                                                                                                                                                            |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `TODOER_AI_ENABLED`                          | Set to `0` for an instance-wide inference stop; `1` allows requests that pass account and model controls.                                                          |
| `TODOER_OPENROUTER_API_KEY`                  | Instance key for managed inference. BYOK uses the requesting user's encrypted key.                                                                                 |
| `TODOER_BYOK_ENCRYPTION_KEY`                 | Dedicated stable encryption material for account OpenRouter keys. Generate and back it up separately. Falls back to the server key if absent.                      |
| `TODOER_BYOK_PREVIOUS_ENCRYPTION_KEY`        | Previous encryption material during a controlled rotation.                                                                                                         |
| `TODOER_ASSIST_PROMPT_FILE`                  | Optional private runtime JSON file with nonempty `classify` and `draft` string fields. It must be readable by the app user.                                        |
| `TODOER_BILLING_ENABLED`                     | New purchases require `1`; default is disabled.                                                                                                                    |
| `TODOER_STRIPE_SECRET_KEY`                   | Stripe server API key (standard or restricted); use test mode for integration verification.                                                                       |
| `TODOER_STRIPE_WEBHOOK_SECRET`               | Signing secret for the configured webhook endpoint.                                                                                                                |
| `TODOER_BILLING_PAYMENT_FEE_BPS`             | Published processing allowance percentage in basis points; default `0`, maximum `5000`.                                                                            |
| `TODOER_BILLING_PAYMENT_FEE_FIXED_MICRO_EUR` | Fixed processing allowance in micro-EUR; default `0`, maximum EUR 1.                                                                                               |
| `TODOER_BILLING_CURRENCY_RATES`              | Optional JSON mapping of lowercase `usd`, `gbp`, `cad` to currency units per EUR scaled by one million. EUR remains fixed. Only configured currencies are offered. |

Use a dedicated random BYOK encryption key rather than changing the server key to rotate BYOK. During rotation, retain the previous key securely, configure both materials, restart the app, and use the admin rotation action in bounded batches until no old records remain. Preserve recovery materials for older backups. Removing the old key too early makes those ciphertexts unreadable.

Private prompt files belong outside Git and image build contexts. Mount the file read-only at runtime and set `TODOER_ASSIST_PROMPT_FILE` to its container path. No prompt contents are included in these instructions. Validation and authorization remain enforced by code regardless of prompt configuration.

## Model readiness and administration

The seeded model entries start disabled. They are configuration candidates, not a claim that a live model was evaluated. Before enabling one, verify availability, feature/schema compatibility, privacy settings, quality and the current provider rates. Configure the provider currency conversion and rate card before enabling managed credit usage. The model list is deliberately bounded; model or provider changes require an explicit operator decision.

A platform administrator sees **Admin** in the account menu. Workspace ownership and an agent credential's admin permission do not grant platform administration. Admin routes require browser authentication; sensitive changes require recent sign-in and CSRF protection.

The **ADMIN** navigation separates Finance (`/admin/finance`), AI access (`/admin/access`), Models & pricing (`/admin/models`), Free credits (`/admin/credits`), Operations (`/admin/operations`) and Audit history (`/admin/audit`). These paths are on your app instance, not this documentation site. Each refresh button fetches current data for that screen without changing settings.

The Free credits screen includes a dedicated **Grant free credits** action requiring an account, positive amount, unique reference and reason. Reusing the same reference and exact details is idempotent. Grants are promotional balance, not a payment or revenue. Positive manual credit adjustments are also recorded as promotional grants. The panel manages access requests, account eligibility and spend caps, provider/model availability, prospective pricing, user balances, usage economics, audited credit adjustments and reconciliation. Model overrides can inherit the provider's configured gross-margin rule. Public user responses contain retail rates, not internal provider expenses or margins.

For a lost Stripe response, the admin recovery form can verify and reconcile an existing PaymentIntent against its local purchase using explicit identifiers and an audit reason; it never creates another charge.

Per-account billing controls let administrators block or restore billing after reviewing payment problems. Forgiving unpaid debt is a separate, explicit write-off action requiring a reason and acknowledgement; it does not refund a payment or erase usage history.

The finance overview starts with four figures: money received after refunds, AI and Stripe costs, profit, and unused purchased credits. Unused credits are not profit yet. Expand the free-credit, payment, profit and model breakdowns for the underlying figures. Promotional grants never count as sales; their inference costs are included in total costs. Unknown costs leave profit marked as still being confirmed. Date filters apply to activity; wallet balances show current totals. BYOK provider charges are paid directly by users and are excluded from Todoer's costs and earnings. This operational dashboard is not a tax or audited financial statement.

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
