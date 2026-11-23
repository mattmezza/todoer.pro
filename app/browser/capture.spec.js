import { test, expect } from '@playwright/test';
for (const viewport of [
  { width: 360, height: 800 },
  { width: 1440, height: 900 },
]) {
  test(`global native capture ${viewport.width}px preserves context and description`, async ({
    page,
  }) => {
    await page.setViewportSize(viewport);
    await page.goto('/signin');
    await page.getByRole('button', { name: 'Enter development workspace' }).click();
    await page.goto('/lists/groceries');
    await page.goto('/capture?list=groceries');
    await expect(page.getByLabel('Destination', { exact: true })).toHaveValue(
      'groceries',
    );
    const title = `Synthetic native capture ${viewport.width} ${Date.now()}`;
    await page.getByLabel('Task', { exact: true }).fill(title);
    await page.getByText('Add a description', { exact: true }).click();
    await page
      .getByLabel('Description, Markdown supported')
      .fill('Preserved **brief** with #Inbox reference.');
    await page.getByRole('button', { name: 'Add task', exact: true }).click();
    await expect(page).toHaveURL(/\/lists\/[^/?]+$/);
    await expect(page.locator('body')).toHaveAttribute('data-list', 'groceries');
    await expect(page.getByRole('link', { name: title, exact: true })).toBeVisible();
    const task = page
      .locator('.task')
      .filter({ has: page.getByRole('link', { name: title, exact: true }) });
    await expect(task.locator('strong')).toHaveText('brief');
    const inboxLink = task.getByRole('link', { name: /inbox/i });
    const inboxPage = await page.request.get(await inboxLink.getAttribute('href'));
    expect(await inboxPage.text()).toContain('data-list="inbox"');
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    ).toBe(true);
  });
}

test('browser capture survives sign-in and saves the reviewed title as typed', async ({
  page,
}) => {
  const title = `Read tomorrow #Inbox ${Date.now()}`;
  const url = 'https://example.com/article?list=one#section';
  const selection = 'Selected words from the article';
  const requests = [];
  page.on('request', (request) => requests.push(request.url()));
  const draft = new URLSearchParams({ title, url, selection });
  await page.goto(`/capture?list=groceries#todoer-capture&${draft}`);
  await expect(page).toHaveURL(/\/signin\?next=[^#]+$/);
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await expect(page).toHaveURL(/\/capture\?list=groceries$/);
  await expect(page.getByLabel('Task', { exact: true })).toHaveValue(title);
  await expect(page.getByLabel('Destination', { exact: true })).toHaveValue('groceries');
  await expect(page.getByLabel('Description, Markdown supported')).toHaveValue(
    `${url}\n\n${selection}`,
  );
  expect(requests.some((request) => request.includes('todoer-capture'))).toBe(false);
  await page.getByRole('button', { name: 'Add task', exact: true }).click();
  await expect(page.locator('body')).toHaveAttribute('data-list', 'groceries');
  await expect(page.getByRole('link', { name: title, exact: true })).toBeVisible();
});

test('a pinned browser list survives sign-in and unavailable lists open Inbox', async ({
  page,
}) => {
  await page.goto('/lists/groceries?browser=1');
  await expect(page).toHaveURL(/\/signin\?next=/);
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await expect(page.locator('body')).toHaveAttribute('data-list', 'groceries');
  await page.goto('/lists/unavailable-browser-pin?browser=1');
  await expect(page).toHaveURL(/\/inbox$/);
});
