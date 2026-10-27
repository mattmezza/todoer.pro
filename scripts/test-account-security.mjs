// Real browser + SMTP + SQLite + WebAuthn virtual authenticator acceptance test.
// Run with local Mailpit and deploy/local/auth-compose.yml --profile tests up -d.
import { chromium } from '../node_modules/playwright/index.mjs';
import { spawn } from 'node:child_process';
import { mkdtemp, rm } from 'node:fs/promises';
import { createHmac } from 'node:crypto';
import path from 'node:path';
const base = 'https://localhost:8445',
  email = `security-${Date.now()}@example.invalid`,
  password = 'correct horse battery staple local test';
const guest = `guest-${Date.now()}@example.invalid`;
const folder = await mkdtemp(path.resolve('.local/rebuild/account-acceptance-'));
let server, browser;
const assert = (value, message) => {
  if (!value) throw Error(message);
};
const pause = (ms) => new Promise((r) => setTimeout(r, ms));
async function start() {
  const env = Object.fromEntries(
    Object.entries(process.env).filter(([k]) => !k.startsWith('TODOER_')),
  );
  Object.assign(env, {
    TODOER_DEV_AUTH: '0',
    TODOER_RUST_DB: path.join(folder, 'test.db'),
    TODOER_PUBLIC_URL: base,
    TODOER_RUST_BIND: '127.0.0.1',
    TODOER_RUST_PORT: '8095',
    TODOER_GOOGLE_LOGIN_ENABLED: 'false',
    TODOER_SIGNUP_ENABLED: 'true',
    TODOER_SERVER_KEY: 'a'.repeat(64),
    TODOER_REGISTRATION_ALLOWLIST: email + ',' + guest,
    TODOER_AUTH_ADMIN_EMAILS: email,
    TODOER_SMTP_HOST: '127.0.0.1',
    TODOER_SMTP_PORT: '1025',
    TODOER_SMTP_INSECURE_LOCAL: '1',
    TODOER_SMTP_FROM: 'Todoer <test@todoer.invalid>',
  });
  server = spawn('target/debug/todoer-server', [], {
    env,
    stdio: ['ignore', 'ignore', 'pipe'],
  });
  let errors = '';
  server.stderr.on('data', (chunk) => {
    errors += chunk;
  });
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch('http://127.0.0.1:8095/ready')).ok) return;
    } catch {}
    if (server.exitCode !== null) throw Error(`Test server exited: ${errors}`);
    await pause(100);
  }
  throw Error('Test server did not start');
}
async function stop() {
  if (!server || server.exitCode !== null) return;
  await new Promise((resolve) => {
    server.once('exit', resolve);
    server.kill('SIGTERM');
  });
}
async function message(pattern, recipient = email) {
  for (let i = 0; i < 80; i++) {
    const data = await (await fetch('http://127.0.0.1:8025/api/v1/messages')).json();
    for (const row of data.messages || []) {
      if (!row.To?.some((t) => t.Address === recipient)) continue;
      const full = await (
        await fetch(`http://127.0.0.1:8025/api/v1/message/${row.ID}`)
      ).json();
      const match = full.Text?.match(pattern);
      if (match) return match[1];
    }
    await pause(150);
  }
  throw Error('Expected account email not captured');
}
function totp(secret, skew = 0) {
  let bits = '';
  for (const c of secret)
    bits += 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'.indexOf(c).toString(2).padStart(5, '0');
  const key = Buffer.from(bits.match(/.{8}/g).map((v) => parseInt(v, 2)));
  const counter = Buffer.alloc(8);
  counter.writeBigUInt64BE(BigInt(Math.floor(Date.now() / 30000) + skew));
  const digest = createHmac('sha1', key).update(counter).digest();
  const offset = digest[19] & 15;
  return ((digest.readUInt32BE(offset) & 0x7fffffff) % 1000000)
    .toString()
    .padStart(6, '0');
}
try {
  await start();
  browser = await chromium.launch();
  const context = await browser.newContext({ ignoreHTTPSErrors: true });
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  const post = (url, data, method = 'POST') =>
    context.request.fetch(base + url, {
      method,
      headers: { Origin: base, 'Content-Type': 'application/json' },
      data,
    });
  await page.goto(base + '/signin');
  const passkeyButton = page.locator('[data-auth-passkey]');
  assert(!(await passkeyButton.isVisible()), 'Passkey button shown without email');
  await page.getByLabel('Email', { exact: true }).fill('invalid');
  assert(!(await passkeyButton.isVisible()), 'Passkey button shown with invalid email');
  await page.getByLabel('Email', { exact: true }).fill(email);
  assert(await passkeyButton.isVisible(), 'Passkey button missing for valid email');
  await page.getByLabel('Email', { exact: true }).fill('');
  assert(
    !(await passkeyButton.isVisible()),
    'Passkey button stayed visible after clearing email',
  );
  await page.goto(base + '/signup');
  assert(
    (await page.locator('.auth-google').count()) === 0,
    'disabled Google button visible',
  );
  await page.getByLabel('Name', { exact: true }).fill('Synthetic Security User');
  await page.getByLabel('Email', { exact: true }).fill(email);
  await page.getByLabel('Password', { exact: true }).fill(password);
  await page.getByRole('button', { name: 'Create account', exact: true }).click();
  await page
    .locator('[data-auth-notice]')
    .filter({ hasText: 'verification link' })
    .waitFor();
  assert(!await page.locator('[data-auth-form]').isVisible(), 'Signup form should disappear after success');
  assert(await page.locator('.auth-confirmation use').getAttribute('href') === '/assets/icons.svg#inbox', 'Signup confirmation needs its inbox icon');
  const token = await message(/verify-email#token=([a-f0-9]+)/);
  await page.goto(base + '/verify-email#token=' + token);
  await page.getByRole('button', { name: 'Verify email & create account' }).click();
  await page.waitForURL('**/inbox');
  assert(
    (await post('/auth/local/verify', { token })).status() !== 200,
    'verification replay accepted',
  );
  const guestContext = await browser.newContext({ ignoreHTTPSErrors: true });
  const guestPost = (url, data, headers = {}) =>
    guestContext.request.post(base + url, {
      headers: { Origin: base, ...headers },
      data,
    });
  assert(
    (
      await guestPost('/auth/local/register', { email: guest, name: 'Guest', password })
    ).ok(),
    'guest registration failed',
  );
  const guestToken = await message(/verify-email#token=([a-f0-9]+)/, guest);
  assert(
    (await guestPost('/auth/local/verify', { token: guestToken })).ok(),
    'guest verification failed',
  );
  assert(
    (await guestContext.request.get(base + '/api/v1/admin/auth-policy')).status() === 403,
    'non-admin read deployment settings',
  );
  const guestCsrf = (await guestContext.cookies()).find(
    (c) => c.name === 'todoer_csrf',
  ).value;
  assert(
    (
      await guestPost(
        '/api/v1/account/totp/start',
        {},
        { 'X-CSRF-Token': guestCsrf, Origin: 'https://evil.example' },
      )
    ).status() === 403,
    'wrong origin accepted',
  );
  assert(
    (
      await guestContext.request.put(base + '/api/v1/admin/auth-policy', {
        headers: { Origin: base, 'X-CSRF-Token': guestCsrf },
        data: { google_enabled: false, signup_enabled: false, revision: 1 },
      })
    ).status() === 403,
    'non-admin changed policy',
  );
  await guestContext.close();
  await page.goto(base + '/settings/security');
  await page.locator('[data-account-summary]').filter({ hasText: email }).waitFor();
  assert(
    !(
      await post('/api/v1/account/password', { password: 'another long enough password' })
    ).ok(),
    'missing CSRF accepted',
  );
  await page.locator('[data-totp-start]').click();
  await page.locator('[data-totp-qr]').waitFor();
  await page.locator('[data-totp-qr]').evaluate((image) => image.decode());
  assert(
    await page
      .locator('[data-totp-qr]')
      .evaluate(
        (image) =>
          image.naturalWidth > 0 && image.src.startsWith('data:image/png;base64,'),
      ),
    'Authenticator QR image must render locally',
  );
  await page.locator('[data-totp-form] summary').click();
  await page
    .locator('[data-totp-secret]')
    .filter({ hasText: /[A-Z2-7]+/ })
    .waitFor();
  const seed = await page.locator('[data-totp-secret]').textContent();
  await page.locator('[data-totp-form] input').fill(totp(seed));
  await page.locator('[data-totp-form] button').click();
  await page.locator('[data-recovery-panel]').waitFor({ state: 'visible' });
  let codes = (await page.locator('[data-recovery-codes]').textContent())
    .trim()
    .split('\n');
  assert(codes.length === 10, 'missing recovery codes');
  assert(
    (await context.request.get(base + '/api/v1/account/security')).status() === 401,
    'enrollment did not revoke old session',
  );
  await page.goto(base + '/signin');
  await page.getByLabel('Email', { exact: true }).fill(email);
  await page.getByLabel('Password', { exact: true }).fill(password);
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  await page.waitForURL('**/signin?step=mfa');
  await page
    .getByLabel('Authenticator or recovery code', { exact: true })
    .fill(totp(seed, 1));
  await page.getByRole('button', { name: 'Verify and sign in' }).click();
  await page.waitForURL('**/today');
  assert(
    (await post('/auth/local/login', { email, password })).ok(),
    'recovery sign-in start failed',
  );
  assert(
    (await post('/auth/local/mfa', { code: codes[0] })).ok(),
    'recovery code rejected',
  );
  assert(
    (await post('/auth/local/login', { email, password })).ok(),
    'recovery replay start failed',
  );
  assert(
    !(await post('/auth/local/mfa', { code: codes[0] })).ok(),
    'recovery code reuse accepted',
  );
  // Enroll a real browser WebAuthn credential using Chromium's CTAP2 emulator.
  const cdp = await context.newCDPSession(page);
  await cdp.send('WebAuthn.enable');
  await cdp.send('WebAuthn.addVirtualAuthenticator', {
    options: {
      protocol: 'ctap2',
      transport: 'internal',
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
      automaticPresenceSimulation: true,
    },
  });
  await page.goto(base + '/settings/security');
  await page.locator('[data-account-summary]').filter({ hasText: email }).waitFor();
  await page.getByLabel('Passkey name').fill('Chromium test passkey');
  await page.locator('[data-add-passkey] button').click();
  await page.locator('[data-recovery-panel]').waitFor({ state: 'visible' });
  codes = (await page.locator('[data-recovery-codes]').textContent()).trim().split('\n');
  await page.goto(base + '/signin');
  await page.getByLabel('Email', { exact: true }).fill(email);
  let assertion;
  page.on('request', (r) => {
    if (r.url().endsWith('/auth/passkey/finish')) assertion = r.postDataJSON();
  });
  await page.getByRole('button', { name: 'Sign in with a passkey' }).click();
  await page.waitForURL('**/today');
  assert(assertion, 'WebAuthn assertion missing');
  assert(
    (await post('/auth/passkey/finish', assertion)).status() !== 200,
    'WebAuthn replay accepted',
  );
  await page.goto(base + '/forgot-password');
  await page.getByLabel('Email', { exact: true }).fill(email);
  await page.getByRole('button', { name: 'Send reset link' }).click();
  await page.locator('[data-auth-notice]').filter({ hasText: 'reset link' }).waitFor();
  const reset = await message(/reset-password#token=([a-f0-9]+)/);
  const started = await post('/auth/passkey/start', { reset_token: reset });
  assert(started.ok(), 'reset passkey start failed');
  assert(
    !(
      await post('/auth/local/reset', {
        token: reset,
        password: 'replacement password not yet authorized',
        code: '',
      })
    ).ok(),
    'unverified reset passkey flow bypassed MFA',
  );
  await page.goto(base + '/reset-password#token=' + reset);
  await page
    .getByLabel('New password', { exact: true })
    .fill('replacement correct horse battery staple');
  await page.getByRole('button', { name: 'Verify with a passkey instead' }).click();
  await page
    .locator('[data-auth-notice]')
    .filter({ hasText: 'Passkey verified' })
    .waitFor();
  await page.getByRole('button', { name: 'Save new password' }).click();
  await page.waitForURL('**/signin');
  assert(
    !(
      await post('/auth/local/reset', {
        token: reset,
        password: 'replacement correct horse battery staple',
        code: codes[0],
      })
    ).ok(),
    'reset replay accepted',
  );
  assert(
    (await context.request.get(base + '/api/v1/account/security')).status() === 401,
    'reset did not revoke session',
  );
  await page.getByLabel('Email', { exact: true }).fill(email);
  await page
    .getByLabel('Password', { exact: true })
    .fill('replacement correct horse battery staple');
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  await page.waitForURL('**/signin?step=mfa');
  await page.getByRole('button', { name: 'Use a passkey' }).click();
  await page.waitForURL('**/today');
  await page.goto(base + '/settings/security');
  await page.locator('[data-auth-policy] button').waitFor({ state: 'visible' });
  await page.waitForFunction(
    () => !document.querySelector('[data-auth-policy] button').disabled,
  );
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const headers = {
    Origin: base,
    'Content-Type': 'application/json',
    'X-CSRF-Token': csrf,
  };
  const original = await (
    await context.request.get(base + '/api/v1/admin/auth-policy')
  ).json();
  const cannotEnable = await context.request.put(base + '/api/v1/admin/auth-policy', {
    headers,
    data: { ...original, google_enabled: true },
  });
  assert(!cannotEnable.ok(), 'unconfigured Google enabled');
  await page.locator('[data-auth-policy] input[name=signup_enabled]').uncheck();
  await Promise.all([
    page.waitForResponse(
      (r) =>
        r.url().endsWith('/api/v1/admin/auth-policy') &&
        r.request().method() === 'PUT' &&
        r.ok(),
    ),
    page.locator('[data-auth-policy] button').click(),
  ]);
  const stale = await context.request.put(base + '/api/v1/admin/auth-policy', {
    headers,
    data: original,
  });
  assert(!stale.ok(), 'stale policy update accepted');
  await page.goto(base + '/signup');
  assert(
    (await page.locator('[data-auth-form=register]').count()) === 0,
    'closed signup form visible',
  );
  assert(
    !(await post('/auth/local/register', { email, name: 'No account', password })).ok(),
    'closed registration accepted',
  );
  assert(
    (await context.request.get(base + '/auth/google', { maxRedirects: 0 })).status() ===
      403,
    'disabled Google endpoint accepted',
  );
  await stop();
  await start();
  await page.goto(base + '/signup');
  assert(
    (await page.locator('[data-auth-form=register]').count()) === 0,
    'environment overwrote saved signup toggle',
  );
  assert(errors.length === 0, errors.join(';'));
  console.log(
    'PASS: verified signup, password login, MFA, recovery, reset with WebAuthn, replay/CSRF checks, admin toggles and restart persistence',
  );
} finally {
  await browser?.close();
  await stop();
  await rm(folder, { recursive: true, force: true });
}
