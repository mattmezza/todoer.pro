import { confirmAction } from './confirm.js';
import { request } from './request.js';

// Retain the exact body/key through quote acceptance; never retry an uncertain dispatch.
export async function inference(path, payload, csrf) {
  const body = JSON.stringify(payload);
  const send = async (quote) => {
    const response = await request('/api/v1/assist/' + path, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-CSRF-Token': csrf,
        ...(quote ? { 'X-Todoer-AI-Quote': quote } : {}),
      },
      body,
    });
    const value = await response.json();
    if (!response.ok)
      throw Error(
        value.error?.message || 'AI request failed. Check usage before trying again.',
      );
    return value;
  };
  const value = await send();
  if (value.status !== 'quote') return value;
  const quote = value.quote;
  const cost =
    quote.funding === 'byok'
      ? `Todoer charge: EUR 0. OpenRouter estimated maximum: ${quote.provider_estimated_max_micro_usd == null ? 'unavailable; check the model rates in your OpenRouter account' : 'USD ' + (quote.provider_estimated_max_micro_usd / 1000000).toFixed(6)}. Your OpenRouter key will be charged directly; Todoer credits will not be used.`
      : `Maximum Todoer charge: EUR ${(quote.max_micro / 1000000).toFixed(6)}. The final charge depends on token usage.`;
  const accepted = await confirmAction(
    `Model: ${quote.model_id}. Inference through OpenRouter. ${cost} Only the selected task or brief and relevant context will be sent.`,
    { title: 'Confirm AI request', acceptLabel: 'Run AI' },
  );
  if (!accepted) throw Error('AI request cancelled. No inference was sent.');
  return send(quote.id);
}
