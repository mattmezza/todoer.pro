import { request as fetch } from './request.js';
import { toast } from './toast.js';
import { confirmAction } from './confirm.js';
import { passkey } from './passkeys.js';
const root = document.querySelector('[data-account-security]');
const csrf = document.querySelector('meta[name=csrf-token]').content;
let busy = false,
  setupId = '',
  policy,
  reauth = false;
async function api(path, body, method = 'POST') {
  const response = await fetch(path, {
    method,
    headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': csrf },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  const data = await response.json();
  if (!response.ok) throw Error(data.error?.message || 'Request failed');
  return data;
}
function showCodes(codes) {
  if (!codes) return;
  root.querySelector('[data-recovery-panel]').hidden = false;
  root.querySelector('[data-recovery-codes]').textContent = codes.join('\n');
}
async function run(fn) {
  if (busy) return;
  busy = true;
  const previous = new Map(
    [
      ...document.querySelectorAll(
        '[data-account-security] button,[data-auth-policy] button',
      ),
    ].map((b) => [b, b.disabled]),
  );
  previous.forEach((_, b) => (b.disabled = true));
  try {
    await fn();
  } catch (e) {
    toast(
      e.name === 'NotAllowedError'
        ? 'Passkey request cancelled or unavailable.'
        : e.message,
      'error',
    );
  } finally {
    busy = false;
    previous.forEach((disabled, b) => {
      if (b.isConnected) b.disabled = disabled || reauth;
    });
  }
}
function redirect(data) {
  if (data.redirect) {
    location.assign(data.redirect);
    return true;
  }
  return false;
}
async function load() {
  const data = await api('/api/v1/account/security', undefined, 'GET');
  root.querySelector('[data-account-summary]').textContent =
    `${data.email} · ${data.has_password ? 'Password enabled' : 'Google / passkey account'} · ${data.recovery_codes_remaining} recovery codes remaining`;
  root.querySelector('[data-totp-start]').hidden = data.totp_enabled;
  root.querySelector('[data-totp-disable]').hidden = !data.totp_enabled;
  const list = root.querySelector('[data-passkey-list]');
  list.replaceChildren();
  for (const key of data.passkeys) {
    const row = document.createElement('div');
    row.className = 'security-key-row';
    const label = document.createElement('span');
    label.textContent = key.label || 'Passkey';
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = 'Remove';
    button.onclick = () =>
      run(async () => {
        if (await confirmAction('Remove this passkey? You will need to sign in again.'))
          redirect(await api('/api/v1/account/passkeys/remove', { id: key.id }));
      });
    row.append(label, button);
    list.append(row);
  }
}
root?.querySelector('[data-set-password]').addEventListener('submit', (event) => {
  event.preventDefault();
  run(async () => {
    if (await confirmAction('Save this password and sign out all sessions?'))
      redirect(
        await api(
          '/api/v1/account/password',
          Object.fromEntries(new FormData(event.target)),
        ),
      );
  });
});
root?.querySelector('[data-totp-start]').addEventListener('click', () =>
  run(async () => {
    const setup = await api('/api/v1/account/totp/start', {});
    setupId = setup.setup_id;
    root.querySelector('[data-totp-qr]').src = `data:image/png;base64,${setup.qr}`;
    root.querySelector('[data-totp-secret]').textContent = setup.secret;
    root.querySelector('[data-totp-form]').hidden = false;
  }),
);
root?.querySelector('[data-totp-form]').addEventListener('submit', (event) => {
  event.preventDefault();
  run(async () => {
    const result = await api('/api/v1/account/totp/finish', {
      setup_id: setupId,
      code: event.target.elements.code.value,
    });
    reauth = true;
    showCodes(result.recovery_codes);
    event.target.hidden = true;
    root.querySelector('[data-totp-qr]').removeAttribute('src');
    root.querySelector('[data-totp-secret]').textContent = '';
    toast('Authenticator enabled. Save your recovery codes, then sign in again.');
  });
});
root?.querySelector('[data-totp-disable]').addEventListener('click', () =>
  run(async () => {
    if (await confirmAction('Remove the authenticator and sign out all sessions?'))
      redirect(await api('/api/v1/account/totp/disable', {}));
  }),
);
root?.querySelector('[data-recovery-generate]').addEventListener('click', () =>
  run(async () => {
    if (await confirmAction('Replace all existing recovery codes?'))
      showCodes((await api('/api/v1/account/recovery-codes', {})).recovery_codes);
  }),
);
root?.querySelector('[data-add-passkey]').addEventListener('submit', (event) => {
  event.preventDefault();
  run(async () => {
    const start = await api(
      '/api/v1/account/passkeys/start',
      Object.fromEntries(new FormData(event.target)),
    );
    const credential = await passkey(start.options, true);
    const result = await api('/api/v1/account/passkeys/finish', {
      challenge_id: start.challenge_id,
      credential,
    });
    reauth = true;
    showCodes(result.recovery_codes);
    if (!result.recovery_codes) location.assign('/signin?next=%2Fsettings%2Fsecurity');
    else toast('Passkey added. Save your recovery codes, then sign in again.');
  });
});
const policyForm = document.querySelector(
  '[data-auth-policy-panel]:not([hidden]) [data-auth-policy]',
);
if (policyForm) {
  const fields = [policyForm.elements.google_enabled, policyForm.elements.signup_enabled];
  fields.forEach((f) => (f.disabled = true));
  policyForm.querySelector('button').disabled = true;
  api('/api/v1/admin/auth-policy', undefined, 'GET')
    .then((data) => {
      policy = data;
      fields.forEach((f) => {
        f.checked = data[f.name];
        f.disabled = false;
      });
      policyForm.querySelector('button').disabled = false;
    })
    .catch((e) => toast(e.message, 'error'));
  policyForm.addEventListener('submit', (event) => {
    event.preventDefault();
    run(async () => {
      policy = await api(
        '/api/v1/admin/auth-policy',
        {
          google_enabled: fields[0].checked,
          signup_enabled: fields[1].checked,
          revision: policy.revision,
        },
        'PUT',
      );
      toast('Authentication settings saved.');
    });
  });
}
if (root) load().catch((e) => toast(e.message, 'error'));
