import { navigate, refreshPage } from './page-navigation.js';
import { beginLoading } from './loading.js';

import { toast } from './toast.js';
import { confirmAction } from './confirm.js';
import { passkey } from './passkeys.js';
import { onPageLoad } from './page-lifecycle.js';

onPageLoad((scope) => {
  if (!document.querySelector('[data-account-security]')) return;
  const fetch = scope.request;
  const root = document.querySelector('[data-account-security]');
  const csrf = document.querySelector('meta[name=csrf-token]').content;
  let activeSection = '';
  root?.querySelectorAll('.security-card').forEach((card, i) => {
    card.id = `security-section-${i}`;
  });
  scope.listen(
    root,
    'click',
    (event) => {
      activeSection = event.target.closest('.security-card')?.id || '';
    },
    true,
  );
  scope.listen(
    root,
    'submit',
    (event) => {
      activeSection = event.target.closest('.security-card')?.id || '';
    },
    true,
  );
  scope.listen(root?.querySelector('[data-download-recovery]'), 'click', () => {
    const codes = root.querySelector('[data-recovery-codes]').textContent;
    if (!codes.trim()) return;
    const blob = new Blob(
      [
        `Todoer recovery codes\n\nKeep these codes somewhere safe. Each code can be used once.\n\n${codes}\n`,
      ],
      { type: 'text/plain;charset=utf-8' },
    );
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = 'todoer-recovery-codes.txt';
    document.body.append(link);
    link.click();
    link.remove();
    scope.timeout(() => URL.revokeObjectURL(url), 1000);
  });
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
    if (!response.ok) {
      const error = Error(data.error?.message || 'Request failed');
      error.code = data.error?.code;
      throw error;
    }
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
      if (e.code === 'reauthentication_required' || e.code === 'unauthorized') {
        if (
          await confirmAction(
            'For your security, sign in again before changing these settings. You’ll return here to complete your action.',
            { title: 'Sign in again', acceptLabel: 'Sign in now' },
          )
        ) {
          const next =
            location.pathname +
            location.search +
            (activeSection ? `#${activeSection}` : location.hash);
          location.assign('/signin?next=' + encodeURIComponent(next));
        }
        return;
      }
      toast(
        e.name === 'NotAllowedError'
          ? 'Passkey request cancelled or unavailable.'
          : e.message,
        'error',
      );
    } finally {
      busy = false;
      previous.forEach((disabled, b) => {
        if (b.isConnected)
          b.disabled = (disabled || reauth) && !b.matches('[data-download-recovery]');
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
    const currentLabel = root.querySelector('[data-current-password]');
    currentLabel.hidden = !data.has_password;
    currentLabel.querySelector('input').required = data.has_password;
    root.querySelector('[data-open-password]').disabled = false;
    root.querySelector('[data-open-password]').textContent = data.has_password
      ? 'Change password…'
      : 'Set password…';
    root.querySelector('#password-heading').textContent = data.has_password
      ? 'Change password'
      : 'Set password';
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
  const passwordDialog = root?.querySelector('[data-password-dialog]');
  const passwordForm = root?.querySelector('[data-set-password]');
  scope.listen(root?.querySelector('[data-open-password]'), 'click', () => {
    passwordDialog.showModal();
    passwordForm.querySelector('label:not([hidden]) input')?.focus();
  });
  root
    ?.querySelectorAll('[data-close-password]')
    .forEach((button) => scope.listen(button, 'click', () => passwordDialog.close()));
  scope.listen(passwordDialog, 'click', (event) => {
    if (event.target !== passwordDialog) return;
    const box = passwordDialog.getBoundingClientRect();
    if (
      event.clientX < box.left ||
      event.clientX > box.right ||
      event.clientY < box.top ||
      event.clientY > box.bottom
    )
      passwordDialog.close();
  });
  scope.listen(passwordDialog, 'close', () => {
    passwordForm.reset();
    passwordForm.elements.password_confirmation.setCustomValidity('');
  });
  scope.listen(passwordForm, 'input', () => {
    const confirmation = passwordForm.elements.password_confirmation;
    confirmation.setCustomValidity(
      confirmation.value && confirmation.value !== passwordForm.elements.password.value
        ? 'New passwords do not match.'
        : '',
    );
  });
  scope.listen(passwordForm, 'submit', async (event) => {
    event.preventDefault();
    if (busy) return;
    busy = true;
    const button = passwordForm.querySelector('[type=submit]');
    button.disabled = true;
    button.textContent = 'Saving…';
    try {
      const result = await api(
        '/api/v1/account/password',
        Object.fromEntries(new FormData(passwordForm)),
      );
      passwordForm.reset();
      redirect(result);
    } catch (error) {
      if (error.code === 'reauthentication_required' || error.code === 'unauthorized') {
        passwordDialog.close();
        if (
          await confirmAction(
            'For your security, sign in again before changing your password. You’ll return here to complete your action.',
            { title: 'Sign in again', acceptLabel: 'Sign in now' },
          )
        ) {
          location.assign(
            '/signin?next=' + encodeURIComponent(location.pathname + '#' + activeSection),
          );
        }
      } else toast(error.message);
    } finally {
      busy = false;
      button.disabled = false;
      button.textContent = 'Save password';
    }
  });
  scope.listen(root?.querySelector('[data-totp-start]'), 'click', () =>
    run(async () => {
      const setup = await api('/api/v1/account/totp/start', {});
      setupId = setup.setup_id;
      root.querySelector('[data-totp-qr]').src = `data:image/png;base64,${setup.qr}`;
      root.querySelector('[data-totp-secret]').textContent = setup.secret;
      root.querySelector('[data-totp-form]').hidden = false;
    }),
  );
  scope.listen(root?.querySelector('[data-totp-form]'), 'submit', (event) => {
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
  scope.listen(root?.querySelector('[data-totp-disable]'), 'click', () =>
    run(async () => {
      if (await confirmAction('Remove the authenticator and sign out all sessions?'))
        redirect(await api('/api/v1/account/totp/disable', {}));
    }),
  );
  scope.listen(root?.querySelector('[data-recovery-generate]'), 'click', () =>
    run(async () => {
      if (await confirmAction('Replace all existing recovery codes?'))
        showCodes((await api('/api/v1/account/recovery-codes', {})).recovery_codes);
    }),
  );
  scope.listen(root?.querySelector('[data-add-passkey]'), 'submit', (event) => {
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
    const fields = [
      policyForm.elements.google_enabled,
      policyForm.elements.signup_enabled,
    ];
    fields.forEach((f) => (f.disabled = true));
    policyForm.querySelector('button').disabled = true;
    const skeleton = beginLoading(policyForm, { variant: 'form' });
    api('/api/v1/admin/auth-policy', undefined, 'GET')
      .then((data) => {
        policy = data;
        fields.forEach((f) => {
          f.checked = data[f.name];
          f.disabled = false;
        });
        policyForm.querySelector('button').disabled = false;
      })
      .catch((e) => {
        skeleton.fail(e.message, () => refreshPage());
        toast(e.message, 'error');
      })
      .finally(() => skeleton.finish());
    scope.listen(policyForm, 'submit', (event) => {
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
  if (root) {
    const skeleton = beginLoading(root, { variant: 'spinner' });
    load()
      .then(() => {
        const section = [...root.querySelectorAll('.security-card')].find(
          (card) => `#${card.id}` === location.hash,
        );
        if (section) {
          section.scrollIntoView({ block: 'center' });
          section.querySelector('input,button')?.focus({ preventScroll: true });
        }
      })
      .catch((e) => {
        skeleton.fail(e.message, () => refreshPage());
        toast(e.message, 'error');
      })
      .finally(() => skeleton.finish());
  }
});
