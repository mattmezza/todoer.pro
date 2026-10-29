// Browser acceptance for the built handbook. Run after `python3 docs/build.py`.
const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const { chromium } = require('@playwright/test');
const path = require('node:path');

(async () => {
  const server = spawn('python3', ['-u', '-m', 'http.server', '0', '--bind', '127.0.0.1', '--directory', path.join(__dirname, '_site')], { stdio: ['ignore', 'pipe', 'ignore'] });
  let browser;
  try {
    const port = await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(Error('Documentation preview did not start')), 10000);
      server.stdout.on('data', (chunk) => {
        const match = chunk.toString().match(/port (\d+)/);
        if (match) { clearTimeout(timer); resolve(match[1]); }
      });
      server.on('error', reject);
      server.on('exit', (code) => { clearTimeout(timer); reject(Error(`Preview exited: ${code}`)); });
    });
    const origin = `http://127.0.0.1:${port}`;
    browser = await chromium.launch({ headless: true });
    const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
    page.setDefaultTimeout(10000);
    const errors = [];
    page.on('pageerror', (error) => errors.push(error.message));
    await page.goto(origin);
    assert.equal(await page.locator('h1').count(), 1);
    await page.getByRole('button', { name: 'Search documentation' }).click();
    await page.getByRole('searchbox', { name: 'Search documentation' }).fill('backup');
    await page.locator('.search-result').first().waitFor();
    await page.keyboard.press('Escape');
    await page.waitForFunction(() => !document.querySelector('dialog').open);
    await page.setViewportSize({ width: 390, height: 844 });
    await page.locator('.mobile-navigation summary').click();
    await page.locator('.mobile-navigation').getByRole('link', { name: 'CLI sign-in', exact: true }).click();
    assert.match(page.url(), /cli-login\.html$/);
    assert.equal(await page.locator('h1').count(), 1);
    await page.goto(`${origin}/rebuild/rest.html`);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false, 'REST guide overflows on mobile');
    await page.goto(origin);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false, 'Home overflows on mobile');
    assert.deepEqual(errors, []);
    const fallback = await browser.newContext({ javaScriptEnabled: false, viewport: { width: 390, height: 844 } });
    const nojs = await fallback.newPage();
    await nojs.goto(`${origin}/getting-started.html`);
    await nojs.locator('.mobile-navigation summary').click();
    await nojs.locator('.mobile-navigation').getByRole('link', { name: 'Deploy with Docker', exact: true }).click();
    assert.match(nojs.url(), /deployment\.html$/);
    console.log('Docs browser checks passed: search, mobile navigation, no-JS navigation, guide layout and no script errors.');
  } finally {
    if (browser) await browser.close();
    server.kill();
  }
})().catch((error) => { console.error(error); process.exitCode = 1; });
