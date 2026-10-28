import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';
test('task and nested subtask references stay consistent across pages, search, and reparenting', async ({ page }) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/lists/inbox');
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  async function api(path, method = 'GET', data) {
    const response = await page.request.fetch(`/api/v1${path}`, { method, data, headers: { 'X-CSRF-Token': csrf, 'Idempotency-Key': randomUUID() } });
    expect(response.ok()).toBeTruthy();
    return response.json();
  }
  const capture = async (title, parent_id = null) => (await api('/capture', 'POST', { list_id: 'inbox', tasks: [{ title, parent_id, description: '' }] }))[0];
  const root = await capture(`Symbols ${randomUUID()}`);
  const child = await capture('Symbols child', root.id);
  const nested = await capture('Symbols nested', child.id);
  for (const [task, prefix] of [[root, '%'], [child, '^'], [nested, '^']]) {
    await page.goto(`/tasks/${task.id}`);
    const copy = page.locator('[data-copy-task-reference]');
    await expect(copy).toHaveText(new RegExp(`^\\${prefix}`));
    const reference = await copy.getAttribute('data-copy-task-reference');
    const matches = await api(`/references/resolve?reference=${encodeURIComponent(reference)}`);
    expect(matches.choices[0].id).toBe(task.id);
    expect(matches.choices[0].prefix).toBe(prefix);
    const hits = await api(`/query?text=${encodeURIComponent(reference)}&status=all`);
    expect(hits.hits[0].reference).toBe(reference);
  }
  const crumbs = page.locator('.breadcrumbs a');
  expect((await crumbs.allTextContents()).filter(value => /^[%^]/.test(value)).map(value => value[0])).toEqual(['%', '^']);
  await page.goto('/lists/inbox');
  await expect(page.locator(`[data-id="${root.id}"] .task-ref`)).toHaveText(/^%/);
  await expect(page.locator(`[data-id="${nested.id}"] .task-ref`)).toHaveText(/^\^/);
  await page.goto(`/tasks/${nested.id}`);
  const current = (await api(`/tasks/${nested.id}/subtree`))[0];
  await api(`/tasks/${nested.id}/reparent`, 'POST', { revision: current.revision, parent_id: null });
  await expect(page.locator('[data-copy-task-reference]')).toHaveText(/^%/, { timeout: 10000 });
  await page.reload();
  await expect(page.locator('[data-copy-task-reference]')).toHaveText(/^%/);
});
