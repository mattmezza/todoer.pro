// Bound requests without retrying writes: a lost response may still have committed.
export async function request(input, options = {}) {
  const method = (options.method || 'GET').toUpperCase();
  const writing = !['GET', 'HEAD', 'OPTIONS'].includes(method);
  const uploading = options.body instanceof Blob || options.body instanceof FormData;
  const deadline = AbortSignal.timeout(uploading ? 300000 : writing ? 45000 : 20000);
  const signal = options.signal ? AbortSignal.any([options.signal, deadline]) : deadline;
  try {
    const response = await globalThis.fetch(input, { ...options, signal });
    if (response.redirected && new URL(response.url).pathname === '/signin')
      throw Error('Your session ended. Sign in again. Your draft has been kept.');
    return response;
  } catch (error) {
    // Lifecycle cancellation is intentional; callers use AbortError to ignore it.
    if (error.name === 'AbortError' && options.signal?.aborted) throw error;
    if (error instanceof TypeError || error.name === 'TimeoutError' || deadline.aborted) {
      throw Error(
        writing
          ? 'The save could not be confirmed. Your draft is kept. Reload saved data before retrying; the server may have received the change.'
          : 'Could not load data. Check your connection and try again.',
      );
    }
    throw error;
  }
}
