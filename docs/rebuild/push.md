# Browser push reminders

The Rust rebuild implements encrypted Web Push for individual scheduled reminders, alongside the existing durable reminder outbox. `/settings/devices` enables a browser after an explicit click, lists registered devices, and supports revocation with a native form. Registration requires a production browser session, a CSRF token and configured VAPID delivery. Development-token and agent-token registration are refused. Browser or OS permission is separate from the account's push opt-in.

## Server configuration

- `TODOER_PUSH_VAPID_PRIVATE_KEY`: persistent P-256 private scalar, 32 bytes encoded as URL-safe base64 without padding. Generate with a trusted Web Push/VAPID key tool and keep it secret; this is not a PEM document. The corresponding public key is derived server-side.
- `TODOER_PUSH_VAPID_SUBJECT`: operator contact, `mailto:…` or HTTPS URL, included in signed VAPID claims.
- `TODOER_PUSH_EXTRA_HOSTS`: optional comma-separated **exact** additional provider hostnames. This explicitly extends the egress allowlist; there are no URL, wildcard, IP-literal, non-443 or HTTP overrides.

Both VAPID variables absent disables delivery; partial or malformed configuration fails initialization. Keep the key stable across deployments: changing it requires browsers to unsubscribe and subscribe with the replacement public key. No push credentials come from object storage, legacy storage, SMTP or Litestream configuration. Endpoint URLs and encryption keys are necessary private subscription credentials stored in SQLite; API listings expose only their SHA256 endpoint fingerprint for browser matching, never the endpoint or encryption secrets. Ordinary backup access therefore also grants access to this sensitive metadata.

Allowed built-in HTTPS providers are `fcm.googleapis.com`, `updates.push.services.mozilla.com`, and subdomains of `push.apple.com`. Validation rejects userinfo, fragments, IP literals and custom ports. Delivery resolves DNS with a timeout, rejects private/local/reserved addresses, pins the checked answers, disables environment proxies and refuses redirects. Test-only loopback delivery is compiled exclusively under `cfg(test)` and is not an environment option.

## Sessions, policy and delivery

Each registration references the exact `auth_sessions.id` that registered it. Expiry, logout or session revocation makes that device ineligible immediately for future sends; signing in again requires explicit re-enablement. Revoking a device disables it server-side even if browser unsubscription fails. Account-wide opt-out disables delivery on every device. Existing notification quiet hours and list mute preferences apply, independently of email opt-in. The existing `Store::reminder_eligible` performs the live reminder lease, schedule, task state and current list permission check; push uses that common eligibility operation rather than a second scheduling rule engine. Delivery rechecks it and the session immediately before the HTTP request.

The shared reminder outbox is claimed by channel: the email worker never consumes push jobs. `push_deliveries` has a unique `(reminder_id,device_id)` row, a bounded-attempt lease, retry availability and terminal sent/dead state. Successful devices are not replayed when another device fails. Subscription revisions fence late responses from an older registration, so an expired response cannot disable a newly re-enabled device. `404`/`410` disables the expired subscription. `429` and server failures retry with bounded `Retry-After` (1–3600 seconds, numeric or HTTP date); other provider rejections become terminal. Network errors use redacted diagnostics. Five attempts per device bound retries, including abandoned leases. Ten devices per account and bounded per-job claim windows keep work within the common 60-second parent lease; unfinished devices resume in another tick. SQLite work runs on a blocking worker, and no database mutex remains held across HTTP I/O.

Payloads use RFC8188 `aes128gcm` encryption and signed VAPID authentication through pinned `web-push 0.11.0`, with existing `reqwest` transport. The payload has exactly these fields:

```json
{"title":"Todoer reminder","url":"/tasks/<task UUID>","intent_id":"<reminder UUID>:<device UUID>"}
```

There is no task title, description, comment, workspace name or list name. The canonical task URL checks current authorization again when opened. Deterministic per-device intent IDs and provider topics allow notification replacement. `TTL: 0` deliberately prevents a provider retaining an offline reminder that could become stale after logout; offline devices can miss reminders. This is a privacy/delivery tradeoff, not guaranteed delivery. An in-flight push accepted before logout and an already displayed notification cannot be recalled. As with email, a crash after provider acceptance but before the SQLite success commit can cause duplicate transport delivery; the stable notification tag reduces duplicate display, and does not establish exactly-once delivery.

Daily and weekly push digests use independent `push_daily_opt_in` and
`push_weekly_opt_in` notification preferences, both false by default. The native
notification settings form exposes each channel separately. Push uses the same
IANA timezone, daily/weekly clock, weekly weekday, quiet hours and per-list mutes
as email. Device/account push opt-in and a live registering session are also
required. Email opt-in never implicitly enables push summaries.

The common digest scheduler and timing policy supply separate
`push_digest_outbox` rows, unique by user/kind/local period. SMTP never claims
these jobs. The existing per-device delivery leases, bounded retries, dead
subscription handling and revision fences operate on `push_digest_deliveries`.
A successful device is not replayed when another device retries. A tick handles
at most one push reminder and one digest, with ten-second device-claim windows
per job plus bounded in-flight network requests. A remaining device resumes on a
later tick. Fresh opt-outs, timing changes, list permissions and sessions are
checked before transport; muted/all-inaccessible accounts receive no digest.
Missed old daily periods are cancelled rather than delivered as catch-up spam.

Digest payloads have the same three-field shape as reminders, with generic title
`Todoer summary` and URL `/review`. They contain no task details or counts. The
review page rechecks current authorization. Device permission,
foreground/background behavior and OS settings remain browser-controlled.

## Browser behavior

The settings controller feature-detects secure contexts, service workers, PushManager and Notifications. It prepares the public key and service worker before exposing the enable button, so `subscribe({userVisibleOnly:true,…})` stays in the direct user gesture. Denied permissions and unavailable server/browser support have explicit explanations. Enabling stores the subscription against the current login session; the browser cannot send arbitrary notification payloads. The disable operation revokes the matching account device first, then unsubscribes in the browser. Native device revocation remains available without JavaScript.

On iOS/iPadOS 16.4 and later, Web Push requires a Home Screen web app; normal browser tabs do not provide the same enablement flow. Safari/macOS and Chromium/Firefox behavior is feature-detected rather than inferred from a browser name. The service worker must display a user-visible notification and handle the canonical same-origin task link. Provider acceptance is not proof the OS displayed it.

### Registration failures and local display test

Browser permission does not guarantee that the browser has a working push
service. In particular, Helium has an [upstream push support
issue](https://github.com/imputnet/helium/issues/705), closed as not planned.
A synthetic localhost probe with the installed Linux Helium browser reproduced
`subscribe()` rejecting with `AbortError: Registration failed - permission denied`.
The old controller silently ignored that browser error as though the page had
been unmounted; it never reached the registration POST. Lifecycle cancellation
is now distinguished using the controller's own abort signal.

Enablement shows connection and save progress. A browser subscription still
pending after 30 seconds produces recovery guidance and releases the button.
Browser subscription operations cannot be cancelled: retry reuses the pending
promise, and a late result is saved only by an actively waiting attempt. Reload
starts a fresh controller. No timeout automatically unsubscribes a device.

**Test notification** uses the active service worker's `showNotification()` when
permission is granted. It checks local browser/OS display, including on browsers
whose remote push service is unavailable. The result explicitly explains that
it does not test server/provider delivery. Browser and OS settings can suppress
notifications even when this API resolves.

The isolated `app/browser/push-registration.spec.js` fixture serves synthetic
HTML, browser API doubles and API responses without an application database.
It covers visible/retryable browser AbortError, a pending subscription timeout
and late completion without duplicate POSTs, and the local display test. Run
`npx playwright test app/browser/push-registration.spec.js`.

## Verification and limits

`cargo test --manifest-path app/Cargo.toml push --lib -- --nocapture` passed the initial reminder tests; later digest tests additionally cover independent opt-in, local-period uniqueness, timing changes, live list/device access and actual encrypted summary HTTP delivery. `cargo clippy --manifest-path app/Cargo.toml --lib --tests -- -D warnings` and `node --check app/assets/push.js` passed. Coverage includes endpoint/SSRF checks, real session expiry/revocation and opt-out eligibility, subscription ownership, device leases, stale lease completion, and production HTTP cookie/CSRF rules. Synthetic local HTTP fixtures capture actual encrypted POST bodies, decrypt using an independently generated receiver key through `ece 2.4.2`, inspect VAPID contact/audience claims and encryption/TTL headers, verify `410` deactivation, honor a `429 Retry-After`, and demonstrate that a successful device is not sent again when another device retries. The fixture checks VAPID structure/claims, not an independent verification of its ECDSA signature.

No actual browser push-provider credentials or personal device were used. Local protocol verification is not evidence of end-to-end Apple, Google or Mozilla device delivery, nor full notification acceptance testing. Real deployment validation must explicitly enable a supported device, schedule a reminder, observe the notification and canonical task open, then confirm logout, permission loss and device revocation stop subsequent sends.

## Official references checked 2026-10-26

- [web-push 0.11.0 API and maintained source](https://docs.rs/web-push/0.11.0/web_push/), including its Sans-I/O message builders; default HTTP clients are disabled.
- [ece 2.4.2](https://docs.rs/ece/2.4.2/ece/) for receiver decryption in tests.
- [MDN PushManager.subscribe](https://developer.mozilla.org/en-US/docs/Web/API/PushManager/subscribe): secure contexts, user-visible subscriptions and application server keys.
- [WebKit Web Push for web apps on iOS and iPadOS](https://webkit.org/blog/13878/web-push-for-web-apps-on-ios-and-ipados/): Home Screen requirements, direct user interaction and `*.push.apple.com` egress.
- [Google's Web Push codelab](https://codelabs.developers.google.com/codelabs/push-notifications): browser subscription and FCM Web Push endpoints.
- [Mozilla Autopush endpoints](https://mozilla-services.github.io/autopush-rs/) and [HTTP protocol](https://mozilla-services.github.io/autopush-rs/http.html): production provider hostname, endpoint invalidation and topics.

Digest increment verification: all eight `push::tests` passed, including actual
local encrypted summary POST/decryption, generic `/review` payload, `429`
partial-device retry without replaying success, `410` deactivation, independent
email/push opt-in, period uniqueness, clock changes, device revocation and live
list authorization/mutes. Four notification tests passed; the separate SMTP
fixture probe was not rerun for this increment. At this snapshot, all-target
Clippy reported only an unrelated in-flight assistance-module diagnostic; no
push/notification diagnostic was reported. Real provider/device checks above
remain unverified.
