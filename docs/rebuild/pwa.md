# PWA and local capture behavior

The manifest has stable identity `/`, standalone display and the existing any/
maskable icons. Assets are self-hosted. Install UI depends on the browser and
platform; no install button is shown when unsupported. On desktop Chromium use
the browser's install option when offered. On Apple devices installation is a
browser share-menu action; actual iOS installation and push are **unverified**.
Session-bound push subscriptions and device management are implemented at
`/settings/devices`. Daily/weekly push opt-ins are separate notification
preferences. Encrypted reminders open the canonical task route; generic summaries
open `/review`. The worker accepts only those application links and displays
fixed generic text. See [push](push.md) for protocol evidence and limits; a local
encrypted HTTP fixture does not establish physical-device delivery.

The app is online first. The service worker stores at most ten recent list pages
for the single active account scope. A cached page is accepted only when its
server `X-Todoer-Account` header matches that scope. Sign-in screens, API/MCP
responses and task-detail mutation responses are never cached. Production OIDC authentication, durable revocable sessions and account-scoped
cache/draft identities are implemented. Local production-mode authentication and
revocation tests are separate from the synthetic browser offline checks below.
A real Google-account switch followed by physical-device offline/cache and push
checks remains unverified; this is an acceptance gap rather than missing auth
code.

Quick captures go to IndexedDB before any network request. The queue stores the
original title, explicit destination, exact payload and idempotency key. Reload
and browser restart preserve them. Replay uses an IndexedDB lease and the same
server idempotency key; overlapping tabs cannot create duplicate logical tasks.
Pending captures display their text and sync state and can be explicitly
removed. Validation/conflict errors require attention rather than infinite
retries. Other failures retain the queue for an online event, manual retry, or the 15-second replay check while the app remains open. Requests time out after 15 seconds.
A crash during an in-flight request may delay replay up to the 60-second lease.
There is no background-sync or delivery guarantee when the app is closed.

Logout revokes the browser session and clears device drafts, capture data,
account page caches and the worker's persisted account scope. If unsynced
captures exist, logout asks before discarding them. Reconnect to revoke a session
when offline. Remote permission revocation cannot erase already cached offline
copies from a disconnected device; reconnect rechecks authorization. Do not
consider offline cached copies a revocation mechanism.

A worker update waits for the user. `Update ready` activates new assets; reload
when drafts are safe. Static cache names are derived at Rust build time from the
bundled asset contents. Old build caches are cleared on activation, not while an
older worker still controls pages. This development workflow requires assets to
be compiled before the Rust build (`make rust-build` does this).

Browser evidence recorded on 2026-10-26 (not rerun by the later documentation audit): Chromium and Firefox desktop engines at emulated
390×844, 820×1180 and 1440×1000 sizes. Tests exercise offline reload, an actual Chromium browser restart through an isolated loopback network outage, and one-time capture replay. These are not physical-device tests. WebKit cannot launch on
this host due to missing ICU66/libxml2/libwebp6/libffi7/enchant libraries; its
installation, push and offline behavior are not verified.

## Android and iOS installation

All full app pages and sign-in declare the shared manifest and Home Screen
metadata. Android has 192px/512px icons plus a maskable icon; Apple has an explicit
180px touch icon. General settings offers installation when the browser exposes
an install prompt and otherwise shows platform instructions. Authenticated pages
register the same account-scoped service worker, including direct navigation to
Today or Settings. Safe-area padding protects the bell and mobile dialogs.

On Android, use Install app / Add to Home Screen. On iOS/iPadOS 16.4 or newer,
use Share → Add to Home Screen, launch that icon, then enable notifications in
Settings → Devices. Push permission remains an explicit user gesture. Testing
on a physical phone requires a reachable HTTPS hostname with a trusted certificate;
`localhost` on the phone refers to the phone itself. The local self-signed desktop
setup does not establish phone compatibility. Test installation, login return,
background push, notification taps and updates on both physical platforms before
claiming device acceptance. Offline support remains limited to the behavior above.
