// Only transport conversion lives here; the server keeps and verifies challenges.
const bytes = value => Uint8Array.from(atob(value.replace(/-/g, '+').replace(/_/g, '/')), c => c.charCodeAt(0));
const base64 = value => btoa(String.fromCharCode(...new Uint8Array(value))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
export async function passkey(options, create = false) {
  if (!window.PublicKeyCredential || !navigator.credentials) throw Error('Passkeys require a supported browser and a trusted HTTPS connection.');
  const publicKey = {...options.publicKey, challenge: bytes(options.publicKey.challenge)};
  if (create) {
    publicKey.user = {...publicKey.user, id: bytes(publicKey.user.id)};
    if (publicKey.excludeCredentials) publicKey.excludeCredentials = publicKey.excludeCredentials.map(c => ({...c, id: bytes(c.id)}));
  } else if (publicKey.allowCredentials) publicKey.allowCredentials = publicKey.allowCredentials.map(c => ({...c, id: bytes(c.id)}));
  const credential = await navigator.credentials[create ? 'create' : 'get']({publicKey});
  if (!credential) throw Error('Passkey request was cancelled.');
  if (credential.toJSON) return credential.toJSON();
  const response = {clientDataJSON: base64(credential.response.clientDataJSON)};
  for (const key of ['attestationObject', 'authenticatorData', 'signature', 'userHandle'])
    if (credential.response[key]) response[key] = base64(credential.response[key]);
  if (credential.response.getTransports) response.transports = credential.response.getTransports();
  return {id:credential.id, rawId:base64(credential.rawId), type:credential.type, response,
    clientExtensionResults:credential.getClientExtensionResults(), authenticatorAttachment:credential.authenticatorAttachment};
}
