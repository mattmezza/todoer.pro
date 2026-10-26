import { test, expect } from '@playwright/test';
import crypto from 'node:crypto';

for (const [name, width, height] of [
  ['phone', 390, 844],
  ['desktop', 1440, 1000],
]) {
  test(`${name}: native workspace, list, section and direct sharing forms`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height });
    await page.goto('/signin');
    await page.getByRole('button', { name: 'Enter development workspace' }).click();
    await page.goto('/workspaces');
    const workspaceTitle = `Household ${crypto.randomUUID().slice(0, 8)}`;
    const create = page.locator('form[action="/workspaces"]');
    await create.locator('[name="title"]').fill(workspaceTitle);
    await create.locator('[name="description"]').fill('A **shared** plan.');
    await create.getByRole('button', { name: 'Create workspace' }).click();
    await expect(
      page.getByRole('heading', { name: workspaceTitle, exact: true }),
    ).toBeVisible();
    const newList = page.locator('form[action$="/lists"]');
    await newList.locator('[name="title"]').fill('Groceries');
    await newList.getByRole('button', { name: 'Create list' }).click();
    await expect(page).toHaveURL(/\/lists\/[a-f0-9-]+\/sharing$/);
    const newSection = page.locator('form[action$="/sections"]');
    await newSection.locator('[name="title"]').fill('Next shop');
    await newSection.getByRole('button', { name: 'Create section' }).click();
    await expect(
      page.getByRole('heading', { name: 'Next shop', exact: true }),
    ).toBeVisible();
    const share = page.locator('form[action$="/sharing/grants"]');
    await share.locator('[name="user_id"]').fill('other');
    await share.locator('[name="role"]').selectOption('editor');
    await share.locator('[name="acknowledge_access_change"]').check();
    await share.getByRole('button', { name: 'Save direct share' }).click();
    await expect(
      page.locator('form[action$="/sharing/revoke"] [name="user_id"][value="other"]'),
    ).toHaveCount(1);
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    ).toBe(true);
    const controls = page.locator(
      'button:visible, select:visible, input:not([type=hidden]):visible',
    );
    for (const control of await controls.all()) {
      if ((await control.getAttribute('type')) === 'checkbox') continue;
      const box = await control.boundingBox();
      expect(box.height).toBeGreaterThanOrEqual(40);
    }
  });
}
