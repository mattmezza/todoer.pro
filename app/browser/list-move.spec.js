import { test, expect } from '@playwright/test';
import crypto from 'node:crypto';
const auth = { Authorization: 'Bearer local-integration-token-not-for-production' };
async function post(request, path, data) {
  const r = await request.post(`/api/v1${path}`, {
    headers: { ...auth, 'Idempotency-Key': crypto.randomUUID() },
    data,
  });
  expect(r.ok(), await r.text()).toBe(true);
  return r.json();
}
async function fixture(request) {
  const name = crypto.randomUUID().slice(0, 8);
  const source = await post(request, '/workspaces', { title: `Source ${name}` });
  const destination = await post(request, '/workspaces', {
    title: `Destination ${name}`,
  });
  const list = await post(request, '/lists', {
    workspace_id: source.id,
    title: `Move ${name}`,
  });
  const section = await post(request, `/lists/${list.id}/sections`, {
    title: 'Keep section',
  });
  const [root] = await post(request, '/capture', {
    list_id: list.id,
    tasks: [{ title: 'Root', description: '', parent_id: null }],
  });
  const [child] = await post(request, '/capture', {
    list_id: list.id,
    tasks: [{ title: 'Nested child', description: '', parent_id: root.id }],
  });
  const r = await request.put(`/api/v1/tasks/${root.id}/section`, {
    headers: auth,
    data: { revision: root.revision, section_id: section.id },
  });
  expect(r.ok(), await r.text()).toBe(true);
  return { source, destination, list, section, root, child };
}
async function openMove(page, f) {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto(`/lists/${f.list.id}/move`);
  await page
    .getByLabel('Destination workspace', { exact: true })
    .selectOption(f.destination.id);
}
for (const [name, width] of [
  ['phone', 390],
  ['desktop', 1440],
]) {
  test(`${name}: native move applies in one step and preserves nested tasks and section assignments`, async ({
    page,
    request,
  }) => {
    await page.setViewportSize({ width, height: 900 });
    const f = await fixture(request);
    await openMove(page, f);
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    ).toBe(true);
    const button = page.getByRole('button', { name: 'Move', exact: true });
    expect((await button.boundingBox()).height).toBeGreaterThanOrEqual(44);
    await button.click();
    await expect(page).toHaveURL(new RegExp(`/lists/${f.list.id}/move$`));
    await expect(page.locator('.toast')).toHaveText('List moved.');
    const moved = await (
      await request.get(`/api/v1/lists/${f.list.id}`, { headers: auth })
    ).json();
    expect(moved.workspace_id).toBe(f.destination.id);
    const tasks = await (
      await request.get(`/api/v1/lists/${f.list.id}/tasks`, { headers: auth })
    ).json();
    expect(tasks.find((t) => t.id === f.child.id).parent_id).toBe(f.root.id);
    for (const task of [f.root, f.child]) {
      const section = await (
        await request.get(`/api/v1/tasks/${task.id}/section`, { headers: auth })
      ).json();
      expect(section.section_id).toBe(f.section.id);
    }
    const stale = await request.patch(`/api/v1/tasks/${f.root.id}`, {
      headers: auth,
      data: { revision: f.root.revision, title: 'Stale' },
    });
    expect(stale.status()).toBe(409);
  });
}
test('API move still rejects a stale snapshot; the form gets a fresh snapshot automatically', async ({
  page,
  request,
}) => {
  const f = await fixture(request);
  const preview = await post(request, `/lists/${f.list.id}/workspace-move/preview`, {
    destination_workspace_id: f.destination.id,
  });
  await post(request, '/capture', {
    list_id: f.list.id,
    tasks: [{ title: 'Added after snapshot', description: '', parent_id: null }],
  });
  const stale = await request.post(`/api/v1/lists/${f.list.id}/workspace-move`, {
    headers: auth,
    data: {
      destination_workspace_id: f.destination.id,
      list_revision: preview.list_revision,
      source_workspace_revision: preview.source_workspace_revision,
      destination_workspace_revision: preview.destination_workspace_revision,
      policy_hash: preview.policy_hash,
      acknowledge_access_change: true,
    },
  });
  expect(stale.status()).toBe(409);
  await openMove(page, f);
  await page.getByRole('button', { name: 'Move', exact: true }).click();
  await expect(page.locator('.toast')).toHaveText('List moved.');
  const current = await (
    await request.get(`/api/v1/lists/${f.list.id}`, { headers: auth })
  ).json();
  expect(current.workspace_id).toBe(f.destination.id);
});
