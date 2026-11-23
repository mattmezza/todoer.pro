import { browserApi } from './client.js';

function base64url(bytes) {
  return btoa(String.fromCharCode(...bytes))
    .replaceAll('+', '-')
    .replaceAll('/', '_')
    .replace(/=+$/, '');
}

async function loginRequest(origin, action, { body, token } = {}) {
  const headers = { Accept: 'application/json' };
  if (body) headers['Content-Type'] = 'application/json';
  if (token) headers.Authorization = `Bearer ${token}`;
  let response;
  try {
    response = await fetch(new URL(`/api/extension/login/${action}`, origin), {
      method: 'POST',
      headers,
      body: JSON.stringify(body),
      credentials: 'omit',
      redirect: 'error',
      cache: 'no-store',
      signal: AbortSignal.timeout(15000),
    });
  } catch {
    throw new Error('Could not reach Todoer. Check the server address and try again.');
  }
  if (response.status === 404 && action === 'start') {
    throw new Error(
      'This server does not offer extension sign-in. Use a REST credential below.',
    );
  }
  if (!response.ok) throw new Error('Todoer could not finish sign-in. Try again.');
  return response.status === 204 ? undefined : response.json();
}

export async function signIn(origin) {
  const redirect = browserApi.identity.getRedirectURL('todoer');
  const verifier = base64url(crypto.getRandomValues(new Uint8Array(32)));
  const state = base64url(crypto.getRandomValues(new Uint8Array(32)));
  const hash = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(verifier));
  const { authorization_url, request_id } = await loginRequest(origin, 'start', {
    body: {
      redirect_uri: redirect,
      challenge: base64url(new Uint8Array(hash)),
      state,
      permission: 'write',
    },
  });
  const authorization = new URL(authorization_url);
  if (
    authorization.origin !== origin ||
    authorization.pathname !== '/cli/authorize' ||
    authorization.searchParams.get('id') !== request_id ||
    !request_id
  ) {
    throw new Error('Todoer returned an invalid authorization URL.');
  }
  authorization.searchParams.set('redirect_uri', redirect);
  let result;
  try {
    result = await browserApi.identity.launchWebAuthFlow({
      url: authorization.href,
      interactive: true,
    });
  } catch {
    throw new Error('Sign-in did not finish. Open the approval window again to retry.');
  }
  const callback = new URL(result);
  const params = callback.searchParams;
  if (
    `${callback.origin}${callback.pathname}` !== redirect ||
    params.get('state') !== state
  ) {
    throw new Error('Todoer returned an invalid sign-in response.');
  }
  if (params.get('error') === 'access_denied') throw new Error('Sign-in was canceled.');
  const code = params.get('code');
  if (!code) throw new Error('Todoer did not approve the extension.');
  const { token } = await loginRequest(origin, 'token', {
    body: { request_id, code, verifier },
  });
  if (typeof token !== 'string' || !token)
    throw new Error('Todoer did not issue a credential.');
  return token;
}

export async function revokeSignIn({ origin, token, managed }) {
  if (managed) await loginRequest(origin, 'revoke', { token });
}
