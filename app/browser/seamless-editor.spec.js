import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';

test('task title editing uses available width and grows only when text needs to wrap', async ({
  page,
}) => {
  await page.route('https://editor.test/**', (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === '/assets/seamless-editor.js')
      return route.fulfill({
        contentType: 'text/javascript',
        body: readFileSync(`app${path}`, 'utf8'),
      });
    return route.fulfill({
      contentType: 'text/html',
      body: `<!doctype html><style>
      main { margin: 24px; } button, textarea { font: 600 36px/1.25 sans-serif; }
      button { padding: 0; border: 0; } form { padding: 0; } textarea { resize: none; overflow: hidden; }
      </style><main><button>two cartons</button><form><textarea hidden rows="1">two cartons</textarea></form></main>`,
    });
  });
  for (const width of [1100, 375]) {
    await page.setViewportSize({ width, height: 800 });
    await page.goto('https://editor.test/');
    const original = await page.evaluate(async () => {
      const { editorAnchor, alignEditorField } =
        await import('/assets/seamless-editor.js');
      const button = document.querySelector('button');
      const anchor = editorAnchor(button);
      button.hidden = true;
      const title = document.querySelector('textarea');
      title.hidden = false;
      alignEditorField(title, anchor, { expand: true });
      return {
        left: anchor.box.left,
        height: anchor.box.height,
        width: anchor.box.width,
      };
    });
    const title = page.locator('textarea');
    await title.fill('two cartons pls');
    const box = await title.boundingBox();
    expect(box.width).toBeGreaterThan(original.width);
    expect(box.x).toBeCloseTo(original.left, 0);
    expect(box.height).toBeLessThan(original.height * 1.5);
    await title.fill('two cartons please '.repeat(25));
    expect((await title.boundingBox()).height).toBeGreaterThan(original.height * 2);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= window.innerWidth,
      ),
    ).toBe(true);
  }
});
