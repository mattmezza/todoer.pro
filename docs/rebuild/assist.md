# Optional capture assistance

Capture is independent of AI. `POST /api/v1/assist/capture` saves one task and an exact original-text receipt before returning. It never calls an external provider. The original UTF-8 text is limited to 10 KiB and also becomes the task description. The first nonempty line becomes the display title; a line over 500 characters is shortened with an ellipsis. Text is never silently split into tasks.

Routing is deterministic: an explicit list ID wins, then accessible literal-prefix rules, then the named destination preference and accessible Inbox fallback. Conflicting matching rules produce a conflict requiring an explicit destination. Missing or ambiguous defaults also require a choice. Rules reference stable list IDs; revoked destinations are skipped. Rules can be managed only by the user's browser session or an unscoped admin credential. Their list IDs remain filtered by live write access.

The standalone Assist screen has been removed. `/assist` and `/settings/assist` redirect to `/settings/ai`, which holds capture preferences and AI instructions. Capture and reviewed AI drafts remain available through the regular task flows and the APIs below.

## Provider configuration and privacy

Assistance is disabled when `TODOER_OPENROUTER_API_KEY` is absent. Capture continues to work. The key is server-only and is never included in HTML, JavaScript, responses, or logs. Production requests use the fixed HTTPS OpenRouter chat completion endpoint; clients cannot supply an endpoint or model.

Self-hosted operators can select classification and drafting models with
`TODOER_ASSIST_CLASSIFY_MODEL` and `TODOER_ASSIST_DRAFT_MODEL`. See the release's
source configuration for defaults. Hosted model selection can change and is not
part of the public pricing offer. Account-level BYOK and prepaid credit purchases
are planned; the operator-wide key described here is available today. Current availability is described on the [pricing page](https://todoer.pro/pricing.html).

Classification sends the saved capture text and at most 20 currently write-authorized destination IDs and short list titles (80 characters each). It does not send other tasks, comments, workspace descriptions, or account briefs. Drafting sends only the submitted brief. Avoid sending information you do not want processed by an external provider.

Requests use strict JSON schema output, `require_parameters: true`, `data_collection: "deny"`, `allow_fallbacks: false`, and provider price caps of $1 per million prompt tokens, $4 per million completion tokens, and $0 per-request charges. These are routing caps, not claims about a model's current price. Data-collection restrictions do not establish a universal zero-retention guarantee; review the selected provider's policies. See the official [structured output documentation](https://openrouter.ai/docs/guides/features/structured-outputs) and [provider routing documentation](https://openrouter.ai/docs/guides/routing/provider-selection).

Classification has a two-second total HTTP timeout and 128 output-token cap; drafting has eight seconds and 4096 tokens. Responses are capped at 64 KiB. Redirects, application retries, provider fallback, tool execution, and model-selected destinations for drafts are disabled. Unknown destination IDs, extra output fields, malformed JSON, truncated completions, and invalid task trees are rejected. Model confidence is never used as authorization or as a reason to apply a suggestion.

## Durable limits

Reservations are committed before dispatch, including serialized request/schema bytes plus an 8192-token wrapper allowance and the maximum output tokens. This deliberately conservative byte-based bound applies to the configured text models. Reserved cost uses the configured provider price caps. Reservations are never refunded, including after timeout, malformed output, failure, process interruption, or success; provider self-reported usage cannot release the budget.

| Scope | Requests/minute | Requests/UTC day | Reserved tokens/day | Reserved cost/day |
|---|---:|---:|---:|---:|
| User identity, across credentials | 6 | 50 | 250,000 | $0.25 |
| Deployment | 60 | 500 | 2,000,000 | $5.00 |

These are fixed conservative deployment limits in this increment. When a budget is exhausted, manual capture remains available. A repeated request key returns the stored result, or a conflict if dispatch was interrupted before completion. It never silently calls the provider again. A new explicit key requests another attempt and reserves another budget. Reusing a key with different text, destination, model request, task revision, or permission context conflicts.

## REST contract

All routes use existing REST authentication and live authorization. Browser writes also require the existing CSRF token and same-origin checks. MCP credentials are not automatically REST credentials.

- `GET /api/v1/assist/status`: enablement and behavior flags.
- `POST /api/v1/assist/capture`: `{idempotency_key,text,list_id:null|ID,default_key:null|KEY}` → `{task,route_source,original_text_preserved:true}`.
- `POST /api/v1/assist/classify`: `{idempotency_key,task_id,revision}` → `{status:"suggestion",task_id,revision,destination:null|{id,title},applied:false}`, or disabled/unavailable status. Current task revision and every suggested destination are rechecked after the HTTP call.
- `POST /api/v1/assist/drafts`: `{idempotency_key,brief,target}` → `{status:"draft",preview,suggested_list_title}`, or disabled/unavailable. `target` is `{kind:"existing_list",list_id}` or `{kind:"new_list",workspace_id,title,description:"",restricted:true}`. Restricted creation requires workspace-owner access; explicitly choosing inherited access requires editor access.
- `POST /api/v1/assist/drafts/{id}/review`: `{revision,payload}` → reviewed preview. `payload` contains `target` and `tasks:[{title,description,parent_index:null|earlierZeroBasedIndex}]`. Review explicitly validates edited content, destination permissions, and the current audience snapshot. Maximum 50 tasks, depth five, 500-character titles, 10-KiB descriptions, and 64-KiB total payload. Dates, schedules, reminders, and priorities are not accepted fields.
- `POST /api/v1/assist/apply`: `{preview_id,revision,expected_hash,idempotency_key}` → `{list_id,tasks}`. A generated preview must first be explicitly reviewed. Review expires after ten minutes. Modified hashes, stale revisions, expired review, changed destination permissions, or an already-applied preview conflict. Only the server-stored reviewed payload executes. Common list/task mutation methods run inside one transaction, including history and idempotency records. A failure rolls back all created tasks and any new list.
- `GET /api/v1/assist/rules`, `PUT /api/v1/assist/rules` with `{prefix,list_id}`, `DELETE /api/v1/assist/rules/{id}`: at most 20 literal case-sensitive prefix rules per user, prefixes 1–100 bytes.

Task and preview records preserve the original input for inspection; subsequent edits never rewrite the original capture receipt or draft brief. Apply retries recheck live access before returning their saved result. No conversational or hidden client state determines a destination.

## Verification and limits of evidence

Tests use an isolated local HTTP fake provider and synthetic text. They verify real request transport, strict routing flags, output validation, visible-only classification, editable review, atomic rollback, stale/expired/hash conflicts, idempotent retries, and durable budget reservations. They do not evaluate a live model's quality, external retention, availability, or actual billing. No production API key or paid provider call is required for these checks.

Reproduce domain and route checks with `cargo test --manifest-path app/Cargo.toml --locked assist --lib`. With an isolated development server on port 8092 and no provider key, run `npx playwright test app/browser/assist.spec.js --project=chromium --project=firefox --workers=1` to verify retired URLs redirect to AI settings without an Assist navigation entry.
