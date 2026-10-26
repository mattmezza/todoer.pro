import { test, expect } from '@playwright/test';
import crypto from 'node:crypto';
const auth = { Authorization: 'Bearer local-integration-token-not-for-production' };
async function post(request, path, data) {
  const response = await request.post(`/api/v1${path}`, {
    headers: { ...auth, 'Idempotency-Key': crypto.randomUUID() },
    data,
  });
  expect(response.ok(), await response.text()).toBe(true);
  return response.json();
}
async function fixture(request) {
  const suffix = crypto.randomUUID().slice(0, 8);
  const workspace = await post(request, '/workspaces', {
    title: `Reference ${suffix}`,
    description: 'A **focused** workspace.',
  });
  const alpha = await post(request, '/lists', {
    workspace_id: workspace.id,
    title: `Alpha ${suffix}`,
    description: 'First list',
  });
  const beta = await post(request, '/lists', {
    workspace_id: workspace.id,
    title: `Beta ${suffix}`,
  });
  return { workspace, alpha, beta, suffix };
}
async function signIn(page) {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
}
async function noOverflow(page) {
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBe(true);
}
for (const [name, width, height] of [
  ['phone', 360, 800],
  ['desktop', 1440, 1000],
]) {
  test(`${name}: minimal workspace rows, inline editing and real contextual ordering`, async ({
    page,
    request,
  }) => {
    await page.setViewportSize({ width, height });
    const errors = [];
    page.on('pageerror', (error) => errors.push(error.message));
    const f = await fixture(request);
    await signIn(page);
    await page.goto('/workspaces');
    await expect(page.locator('body.workspace-management')).toBeVisible();
    for (let n = 1; n <= 20; n++) {
      await page.goto(`/workspaces?page=${n}`);
      if (await page.getByRole('link', { name: f.workspace.title, exact: true }).count())
        break;
    }
    const link = page.getByRole('link', { name: f.workspace.title, exact: true });
    await expect(link).toHaveCount(1);
    await link.click();
    const heading = page.getByRole('heading', { name: f.workspace.title, exact: true });
    await expect(heading).toBeVisible();
    expect(await heading.evaluate((el) => getComputedStyle(el).fontFamily)).toMatch(
      /mono/i,
    );
    await expect(page.locator('.workspace-description strong')).toHaveText('focused');
    await noOverflow(page);
    const menu = page.locator(
      `.workspace-menu > summary[aria-label="Actions for ${f.alpha.title}"]`,
    );
    expect((await menu.boundingBox()).height).toBeGreaterThanOrEqual(44);
    await menu.click();
    const actions = menu.locator('..');
    await page.keyboard.press('Escape');
    await expect(actions).not.toHaveAttribute('open', '');
    await expect(menu).toBeFocused();
    await menu.click();
    await expect(
      actions.getByRole('link', { name: 'Settings and sections' }),
    ).toHaveAttribute('href', `/lists/${f.alpha.id}/sharing`);
    await actions
      .getByRole('button', { name: `Move ${f.alpha.title} down`, exact: true })
      .click();
    await expect(page.locator('.workspace-row-title').first()).toContainText(
      `beta-${f.suffix}`,
    );
    const alpha = await (
      await request.get(`/api/v1/lists/${f.alpha.id}`, { headers: auth })
    ).json();
    const beta = await (
      await request.get(`/api/v1/lists/${f.beta.id}`, { headers: auth })
    ).json();
    expect(alpha.position).toBeGreaterThan(beta.position);
    const before = await (
      await request.get(`/api/v1/workspaces/${f.workspace.id}`, { headers: auth })
    ).json();
    await page.getByRole('heading', { name: f.workspace.title, exact: true }).click();
    const edit = page.locator('#workspace-settings form');
    await expect(edit.locator('[name=title]')).toBeVisible();
    await edit.locator('[name=title]').fill(`${f.workspace.title} revised`);
    await edit.locator('[name=description]').fill('Saved **inline** context.');
    await edit.getByRole('button', { name: 'Save workspace', exact: true }).click();
    await expect(
      page.getByRole('heading', { name: `${f.workspace.title} revised`, exact: true }),
    ).toBeVisible();
    await expect(page.locator('.workspace-description strong')).toHaveText('inline');
    const after = await (
      await request.get(`/api/v1/workspaces/${f.workspace.id}`, { headers: auth })
    ).json();
    expect(after.revision).toBe(before.revision + 1);
    expect(after.description).toBe('Saved **inline** context.');
    await noOverflow(page);
    await page.screenshot({
      path: `.local/rebuild/reference-workspace-${test.info().project.name}-${width}.png`,
    });
    expect(errors).toEqual([]);
  });

  test(`${name}: account menu stays at the sidebar bottom and opens real destinations`, async ({
    page,
    request,
  }) => {
    await page.setViewportSize({ width, height });
    const f = await fixture(request);
    await signIn(page);
    await page.goto(`/lists/${f.alpha.id}`);
    await page.waitForFunction(() => document.body.dataset.ready === 'true');
    if (width < 768) await page.locator('#nav-toggle').click();
    const sidebar = page.locator('aside.reference-sidebar');
    const summary = sidebar.locator('.account-menu > summary');
    await expect(summary).toBeVisible();
    const box = await summary.boundingBox();
    expect(box.height).toBeGreaterThanOrEqual(44);
    expect(box.y + box.height).toBeLessThanOrEqual(height);
    const sidebarBox = await sidebar.boundingBox();
    expect(sidebarBox.y + sidebarBox.height).toBeLessThanOrEqual(height + 1);
    expect(sidebarBox.y + sidebarBox.height - box.y - box.height).toBeLessThan(30);
    await summary.click();
    const menu = sidebar.locator('.account-menu');
    await expect(menu).toHaveAttribute('open', '');
    const panel = menu.locator('.account-menu-panel');
    await expect(panel).toBeVisible();
    const bounds = await panel.boundingBox();
    expect(bounds.x).toBeGreaterThanOrEqual(0);
    expect(bounds.y).toBeGreaterThanOrEqual(0);
    expect(bounds.x + bounds.width).toBeLessThanOrEqual(width + 1);
    expect(bounds.y + bounds.height).toBeLessThanOrEqual(height + 1);
    await page.screenshot({
      path: `.local/rebuild/reference-account-menu-${test.info().project.name}-${width}.png`,
    });
    await expect(
      panel.getByRole('link', { name: 'Delivery status', exact: true }),
    ).toHaveAttribute('href', '/settings/jobs');
    await summary.focus();
    await page.keyboard.press('Enter');
    await expect(menu).not.toHaveAttribute('open', '');
    await summary.click();
    await panel.getByRole('link', { name: 'Manage spaces', exact: true }).click();
    await expect(page).toHaveURL(/\/workspaces$/);
    await page.waitForLoadState('load');
    await expect(page.locator('body.workspace-management')).toBeVisible();
    if (width < 768) await page.locator('#nav-toggle').click();
    const sharedSummary = page.locator('aside.reference-sidebar .account-menu > summary');
    await expect(sharedSummary).toBeVisible();
    await sharedSummary.click();
    await expect(
      page
        .locator('aside.reference-sidebar .account-menu-panel')
        .getByRole('link', { name: 'Account & agents', exact: true }),
    ).toBeVisible();
    await noOverflow(page);
  });
}

test('phone: long workspace and list names wrap inside minimal rows', async ({
  page,
  request,
}) => {
  await page.setViewportSize({ width: 360, height: 800 });
  const suffix = crypto.randomUUID().replaceAll('-', '');
  const title = `Workspace${suffix}${suffix}`;
  const workspace = await post(request, '/workspaces', { title });
  await post(request, '/lists', {
    workspace_id: workspace.id,
    title: `List${suffix}${suffix}`,
  });
  await signIn(page);
  await page.goto('/workspaces');
  await noOverflow(page);
  for (let n = 1; n <= 20; n++) {
    await page.goto(`/workspaces?page=${n}`);
    if (await page.getByRole('link', { name: title, exact: true }).count()) break;
  }
  await page.getByRole('link', { name: title, exact: true }).click();
  await expect(page.getByRole('heading', { name: title, exact: true })).toBeVisible();
  await noOverflow(page);
});
