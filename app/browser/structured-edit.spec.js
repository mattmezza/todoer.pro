import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
for (const width of [375, 1280]) {
  test(`structured edit previews and preserves literal titles at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 850 });
    await page.route('https://editing.test/**', (route) => {
      const path = new URL(route.request().url()).pathname;
      if (path.startsWith('/assets/'))
        return route.fulfill({
          contentType: 'text/javascript',
          body: readFileSync(`app${path}`, 'utf8'),
        });
      return route.fulfill({
        contentType: 'text/html',
        body: '<input aria-label="Title" value="Call client +work !! :45m"><button id="save">Save</button><output></output>',
      });
    });
    await page.goto('https://editing.test/');
    await page.evaluate(async () => {
      const { mountStructuredEdit } = await import('/assets/structured-edit.js');
      const save = mountStructuredEdit(document.querySelector('input'));
      document.querySelector('#save').onclick = async () => {
        const result = await save(
          async (path, method, data) => ({ path, method, data }),
          'task',
          { revision: 2, title: document.querySelector('input').value },
        );
        document.querySelector('output').textContent = JSON.stringify(result);
      };
    });
    await page.locator('#save').click();
    await expect(page.locator('output')).toContainText('PATCH');
    await page.getByRole('checkbox').check();
    await page.locator('#save').click();
    await expect(page.getByRole('dialog')).toContainText('Add tags: work');
    await page.getByRole('button', { name: 'Save interpretation' }).click();
    await expect(page.locator('output')).toContainText('structured-edit');
    const value = JSON.parse(await page.locator('output').textContent());
    expect(value.data.title).toBe('Call client');
    expect(value.data.metadata.duration_minutes).toBe(45);
  });
}
