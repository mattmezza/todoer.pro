// Optional phone-emulated Chromium acceptance for the synthetic HTTP probe only.
const { chromium, expect } = require('@playwright/test');
const path = require('node:path');

(async () => {
  const [base, output] = process.argv.slice(2);
  if (!/^http:\/\/127\.0\.0\.1:\d+$/.test(base || ''))
    throw new Error('loopback fixture URL required');
  const browser = await chromium.launch({ headless: true });
  try {
    const context = await browser.newContext({
      viewport: { width: 390, height: 844 },
      deviceScaleFactor: 1,
      isMobile: true,
      hasTouch: true,
      colorScheme: 'dark',
      serviceWorkers: 'block',
    });
    const page = await context.newPage();
    const errors = [];
    page.on('pageerror', (error) => errors.push(error.message));
    await page.goto(`${base}/signin`);
    await page.getByRole('button', { name: 'Enter development workspace' }).click();
    await page.waitForFunction(() => document.body.dataset.ready === 'true');
    const measurements = [];
    for (const [list, count] of [
      ['perf-list-000', 100],
      ['perf-stress', 1000],
    ]) {
      const start = performance.now();
      const response = await page.goto(`${base}/lists/${list}`, {
        waitUntil: 'domcontentloaded',
      });
      if (response.status() !== 200) throw new Error('list navigation failed');
      await page.waitForFunction(() => document.body.dataset.ready === 'true');
      await expect(page.locator('#task-list .task')).toHaveCount(count);
      await page.evaluate(
        () =>
          new Promise((resolve) =>
            requestAnimationFrame(() => requestAnimationFrame(resolve)),
          ),
      );
      const readyMs = performance.now() - start;
      const metrics = await page.evaluate(() => {
        const n = performance.getEntriesByType('navigation')[0];
        return {
          viewport: { width: innerWidth, height: innerHeight },
          document_width: document.documentElement.scrollWidth,
          dom_elements: document.querySelectorAll('*').length,
          task_rows: document.querySelectorAll('#task-list .task').length,
          response_end_ms: n.responseEnd,
          dom_content_loaded_ms: n.domContentLoadedEventEnd,
          load_ms: n.loadEventEnd || null,
          js_heap_used_bytes: performance.memory?.usedJSHeapSize ?? null,
        };
      });
      if (metrics.document_width > metrics.viewport.width)
        throw new Error(`${list}: horizontal overflow`);
      const row = page.locator('#task-list .task').first();
      const actionStart = performance.now();
      await row.locator('.task-menu').click();
      await expect(row.locator('.menu-items')).toBeVisible();
      await row.locator('.task-menu').click();
      await row.locator('.task-title').click();
      await expect(page.locator('#task-editor')).toBeVisible();
      await expect(page.locator('#edit-title')).not.toHaveValue('');
      await page.locator('#close-editor').click();
      await expect(page.locator('#task-editor')).not.toBeVisible();
      let completionMs = null;
      if (list === 'perf-stress') {
        const completionStart = performance.now();
        const [completeResponse] = await Promise.all([
          page.waitForResponse(
            (r) => r.request().method() === 'PATCH' && r.url().includes('/api/v1/tasks/'),
          ),
          row.locator('.completion').click(),
        ]);
        if (completeResponse.status() !== 200) throw new Error('completion HTTP failed');
        await expect(row).toHaveAttribute('data-complete', 'true');
        const [reopenResponse] = await Promise.all([
          page.waitForResponse(
            (r) => r.request().method() === 'PATCH' && r.url().includes('/api/v1/tasks/'),
          ),
          row.locator('.completion').click(),
        ]);
        if (reopenResponse.status() !== 200) throw new Error('reopen HTTP failed');
        await expect(row).toHaveAttribute('data-complete', 'false');
        completionMs = performance.now() - completionStart;
      }
      measurements.push({
        list_tasks: count,
        ...metrics,
        ready_two_frames_ms: +readyMs.toFixed(3),
        menu_and_editor_actions_ms: +(performance.now() - actionStart).toFixed(3),
        completion_reopen_roundtrip_ms:
          completionMs === null ? null : +completionMs.toFixed(3),
        actions_passed: true,
        horizontal_overflow: false,
      });
      await page.screenshot({ path: path.join(output, `phone-${count}.png`) });
    }
    if (errors.length)
      throw new Error(`uncaught browser errors: ${JSON.stringify(errors)}`);
    console.log(
      JSON.stringify({
        browser: 'Chromium',
        version: browser.version(),
        phone_emulation: true,
        real_mobile_hardware: false,
        service_workers: 'blocked for direct-server measurement',
        uncaught_errors: errors,
        measurements,
      }),
    );
    await context.close();
  } finally {
    await browser.close();
  }
})().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
