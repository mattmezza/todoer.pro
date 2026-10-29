# AI credits and BYOK implementation plan

Status: planned; no checkout or account-level BYOK is available. Core task management and MCP, CLI and REST access remain free. Current optional assistance uses an operator-configured instance-wide provider key and deployment safety limits. Those limits are not a purchased-credit balance.

## Intended experience

1. Start an account and manage tasks without buying anything. Connecting an external AI agent does not consume Todoer credits; the agent's own service charges are separate.
2. For built-in AI, choose prepaid credits or bring your own provider key. Show the funding source before sending task content to a provider.
3. Show the maximum credit charge before a paid request. Reserve that amount atomically, then settle the actual charge. Never permit a negative spendable balance.
4. Keep capture, manual editing, exports and agent access working when credits run out. Explain how to add credits or switch to BYOK.
5. Show balance, purchases, reservations, charges and adjustments in an account usage view. Include time, operation, status and customer charge without exposing credentials or task content in billing records.
6. BYOK has no Todoer AI usage fee; the user pays the provider. Pricing may evolve as the service grows. Notify users before applying future changes.

## Phase 1: account ledger and usage accounting

Add versioned migrations for account wallets, append-only ledger entries, AI operations, purchase records and webhook event receipts. Use integer monetary/credit units with an explicit currency and scale; never floating point. Specify credit denomination and conversion before launching purchases. Keep all balances derived from the ledger, with an optional transactionally maintained cache verified against it.

Assign a stable operation ID and unique account/idempotency key before dispatch. In one SQLite transaction, verify live access and funding, reserve the maximum charge and record dispatch intent. Concurrent requests must not overspend. Settlement and release are append-only entries with unique operation references. Record usage even when it cannot be charged, for reconciliation.

Replace the current conservative nonrefundable safety reservation with a separate customer billing reservation. Retain deployment rate/spend controls as independent protections. Define cancellation, invalid output, provider errors and timeouts explicitly: do not bill a user for a failed feature result. An uncertain provider outcome must not trigger another paid call automatically. A recovery job reconciles interrupted operations and expires holds according to a documented policy.

Keep the hosted model selection configurable and out of the public offer. The self-hosted source and operator configuration are inspectable; a hosted model choice is not a product guarantee. Internal provider accounting and pricing configuration live outside the public catalog, in secret/config storage and ignored local files.

## Phase 2: Stripe credit purchases

Before implementation, choose pack amounts, currency, tax handling, refund and dispute policies, credit expiry policy, purchase limits and billing contact details. Set up Stripe products/prices and test-mode credentials. Public catalogs contain only purchasable customer offers; no provider costs or commercial calculations.

Create Checkout sessions server-side for authenticated accounts using an allowlist of Stripe price IDs. Bind a purchase record to the account, amount, currency and Stripe session. Use idempotency keys when creating sessions. Restrict return URLs to the configured app origin. A browser redirect never grants credit.

Verify webhook signatures against the exact raw body and enforce a bounded request size. Persist the unique event ID and apply its effects in one transaction. Grant credit only for confirmed successful payments, after checking recorded price, amount, currency, payment state and account binding. Handle duplicate and out-of-order events, delayed payment methods, refunds and disputes. Block repeated credit grants via unique purchase/payment references. Reconcile missed webhooks against Stripe independently.

A refund/dispute must append a linked ledger adjustment. Define what happens if purchased credits have already been spent; do not silently take the wallet negative or repeatedly charge a card. Never store card data in Todoer.

## Phase 3: account-level BYOK

Add browser-only settings to save, replace, validate and remove a provider key. Require authenticated account access and CSRF protection; scoped agent credentials cannot read or administer it. Encrypt keys at rest with a versioned server-side encryption key and a rotation/recovery procedure. API responses expose only configured state, never the key. Logs, exports, task data and billing events must exclude it.

Use a fixed provider endpoint allowlist, no client-controlled URLs, and bounded requests. Resolve BYOK credentials only after checking the requesting identity. Clearly show whether a request uses BYOK or Todoer credits. Never fall back from BYOK to purchased credits without explicit consent. Revocation prevents future dispatch; explain that an already dispatched request cannot be recalled. Existing operator-wide keys remain a separate self-hosted configuration, with documented precedence.

## Phase 4: usage screen and launch

Expose paginated account-only balance and ledger endpoints, and a billing screen for purchases, pending reservations, settled charges, refunds and receipts. Label reporting units and timezone. Surface partial failures and delayed reconciliation. Do not expose raw provider responses or internal accounting through the customer API.

Launch behind disabled-by-default configuration. Add synthetic ledger tests for concurrency, retries, timeout recovery, account isolation and rounding; Stripe signature, duplicate/out-of-order webhook and refund fixtures; BYOK encryption and authorization tests; browser checks for empty balance, interrupted checkout and changing funding sources. No development checks use real payments, real provider requests or a production database.

Before enabling purchases, reconcile test purchases end-to-end, rehearse webhook replay and restore, verify the accounting invariant against the ledger, and publish customer-facing prices and policies. Payment configuration and production enablement are separate operator actions.
