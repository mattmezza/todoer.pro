// Only captures are queued. The payload and idempotency key never change on retry.
const DB = 'todoer-captures-v1';
const STORE = 'captures';
const LEASE_MS = 60_000;
function database() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB, 1);
    request.onupgradeneeded = () =>
      request.result.createObjectStore(STORE, { keyPath: 'id' });
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}
async function transact(mode, action) {
  const db = await database();
  try {
    return await new Promise((resolve, reject) => {
      const tx = db.transaction(STORE, mode);
      let result;
      tx.oncomplete = () => resolve(result);
      tx.onerror = tx.onabort = () =>
        reject(tx.error || new Error('Queue transaction aborted'));
      action(tx.objectStore(STORE), (value) => {
        result = value;
      });
    });
  } finally {
    db.close();
  }
}
function validScope(scope) {
  if (typeof scope !== 'string' || !scope)
    throw new Error('An account scope is required');
}
export async function enqueueCapture(scope, list_id, title) {
  validScope(scope);
  const key = crypto.randomUUID();
  const record = {
    id: `${scope}:${key}`,
    scope,
    key,
    payload: { list_id, tasks: [{ title, description: '', parent_id: null }] },
    createdAt: Date.now(),
    status: 'pending',
    leaseUntil: 0,
  };
  await transact('readwrite', (store, done) => {
    store.add(record);
    done(record);
  });
  return record;
}
export async function listPending(scope) {
  validScope(scope);
  return transact('readonly', (store, done) => {
    const request = store.getAll();
    request.onsuccess = () =>
      done(
        request.result
          .filter((r) => r.scope === scope)
          .sort((a, b) => a.createdAt - b.createdAt || a.key.localeCompare(b.key)),
      );
  });
}
export async function clearAccount(scope) {
  validScope(scope);
  await transact('readwrite', (store) => {
    const request = store.openCursor();
    request.onsuccess = () => {
      const cursor = request.result;
      if (!cursor) return;
      if (cursor.value.scope === scope) cursor.delete();
      cursor.continue();
    };
  });
}
async function claim(scope, id) {
  return transact('readwrite', (store, done) => {
    const request = store.get(id);
    request.onsuccess = () => {
      const record = request.result;
      if (
        !record ||
        record.scope !== scope ||
        record.status !== 'pending' ||
        record.leaseUntil > Date.now()
      )
        return done(null);
      record.leaseUntil = Date.now() + LEASE_MS;
      record.leaseOwner = crypto.randomUUID();
      store.put(record);
      done(record);
    };
  });
}
async function finish(record, success, attention, error) {
  await transact('readwrite', (store) => {
    const request = store.get(record.id);
    request.onsuccess = () => {
      const current = request.result;
      // Another tab may have acquired an expired lease, or logout cleared it.
      if (!current || current.leaseOwner !== record.leaseOwner) return;
      if (success) store.delete(record.id);
      else {
        current.leaseUntil = 0;
        current.status = attention ? 'needs_attention' : 'pending';
        current.error = error;
        delete current.leaseOwner;
        store.put(current);
      }
    };
  });
}
export async function replayCaptures(scope, send) {
  validScope(scope);
  let sent = 0;
  const errors = [];
  for (const candidate of await listPending(scope)) {
    const record = await claim(scope, candidate.id);
    if (!record) continue;
    try {
      const response = await send(record);
      if (response && response.ok === false) {
        const error = new Error(`Capture failed (${response.status})`);
        error.status = response.status;
        throw error;
      }
      await finish(record, true);
      sent++;
    } catch (error) {
      const status = Number(error.status || error.response?.status);
      const attention = [400, 403, 409, 422].includes(status);
      const message = String(error.message || error);
      await finish(record, false, attention, message);
      errors.push({
        key: record.key,
        status: status || null,
        message,
        needs_attention: attention,
      });
      if (!attention) break;
    }
  }
  return { sent, pending: (await listPending(scope)).length, errors };
}
export async function discardCapture(scope, key) {
  validScope(scope);
  await transact('readwrite', (store) => {
    const id = `${scope}:${key}`;
    const request = store.get(id);
    request.onsuccess = () => {
      if (request.result?.scope === scope && request.result.key === key) store.delete(id);
    };
  });
}
