import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';

// Serve only synthetic markup and API responses; no application DB or push provider.
async function fixture(page, mode = 'abort') {
  await page.route('http://127.0.0.1:8092/**', async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path.startsWith('/assets/'))
      return route.fulfill({
        contentType: 'text/javascript',
        body: readFileSync(`app${path}`, 'utf8'),
      });
    if (path === '/api/v1/push/key')
      return route.fulfill({ json: { available: true, public_key: 'BA' } });
    if (path === '/api/v1/push/devices/synthetic') return route.fulfill({ status: 204 });
    if (path === '/api/v1/push/devices')
      return route.fulfill({
        json: route.request().method() === 'POST' ? { id: 'synthetic' } : { devices: [] },
      });
    return route.fulfill({
      contentType: 'text/html',
      body: `<section data-push-settings data-csrf="synthetic"><p data-push-status></p><button data-push-enable hidden>Enable notifications</button><button data-push-disable hidden>Disable</button><button data-push-test hidden>Test notification</button><input data-push-opt-in type="checkbox"><input data-push-label value="Synthetic"></section><script type="module" src="/assets/push.js"></script>`,
    });
  });
  await page.addInitScript((mode) => {
    window.PushManager = class {};
    window.Notification = { permission: 'granted' };
    window.subscribeCalls = 0;
    const subscription = {
      unsubscribe: async () => true,
      toJSON: () => ({ endpoint: 'https://synthetic.invalid/push', keys: {} }),
    };
    let resolve;
    const pending = new Promise((done) => (resolve = done));
    window.finishSubscription = () => resolve(subscription);
    const registration = {
      active: {},
      showNotification: async () => {
        window.testShown = true;
      },
      pushManager: {
        getSubscription: async () => null,
        subscribe: () => {
          window.subscribeCalls++;
          if (mode === 'pending') return pending;
          if (mode === 'success') return Promise.resolve(subscription);
          return Promise.reject(
            new DOMException('Push service unavailable', 'AbortError'),
          );
        },
      },
    };
    Object.defineProperty(navigator, 'serviceWorker', {
      value: { register: async () => registration },
    });
  }, mode);
  await page.goto('http://127.0.0.1:8092/settings/devices');
  await expect(page.locator('[data-push-enable]')).toBeVisible();
}

test('browser AbortError is visible and retryable, without a registration POST', async ({
  page,
}) => {
  let posts = 0;
  page.on('request', (request) => {
    if (request.method() === 'POST') posts++;
  });
  await fixture(page);
  await page.locator('[data-push-enable]').click();
  await expect(page.locator('[data-push-status]')).toContainText(
    'Push service unavailable',
  );
  await expect(page.locator('[data-push-enable]')).toBeEnabled();
  await page.locator('[data-push-enable]').click();
  await expect.poll(() => page.evaluate(() => window.subscribeCalls)).toBe(2);
  expect(posts).toBe(0);
});

test('pending subscription times out and retries reuse it; late result saves once', async ({
  page,
}) => {
  let posts = 0;
  page.on('request', (request) => {
    if (request.method() === 'POST') posts++;
  });
  await fixture(page, 'pending');
  await page.clock.install();
  await page.locator('[data-push-enable]').click();
  await expect(page.locator('[data-push-status]')).toContainText('Connecting');
  await expect(page.locator('[data-push-enable]')).toBeDisabled();
  await page.clock.fastForward(31000);
  await expect(page.locator('[data-push-status]')).toContainText('did not respond');
  await expect(page.locator('[data-push-enable]')).toBeEnabled();
  expect(posts).toBe(0);
  await page.locator('[data-push-enable]').click();
  await page.evaluate(() => window.finishSubscription());
  await expect(page.locator('[data-push-status]')).toHaveText(
    'Notifications are enabled in this browser.',
  );
  expect(await page.evaluate(() => window.subscribeCalls)).toBe(1);
  expect(posts).toBe(1);
});

test('test notification checks local display and explains its scope', async ({
  page,
}) => {
  await fixture(page, 'success');
  await page.locator('[data-push-test]').click();
  await expect(page.locator('[data-push-status]')).toContainText(
    'not server push delivery',
  );
  expect(await page.evaluate(() => window.testShown)).toBe(true);
});

test('failed server save can retry the same subscription', async ({ page }) => {
  await fixture(page, 'success');
  await page.route('**/api/v1/push/devices', (route) =>
    route.fulfill({
      status: 503,
      json: { error: { message: 'Synthetic save failure' } },
    }),
  );
  await page.locator('[data-push-enable]').click();
  await expect(page.locator('[data-push-status]')).toContainText(
    'Synthetic save failure',
  );
  await expect(page.locator('[data-push-enable]')).toBeEnabled();
  await page.unroute('**/api/v1/push/devices');
  await page.locator('[data-push-enable]').click();
  await expect(page.locator('[data-push-status]')).toHaveText(
    'Notifications are enabled in this browser.',
  );
  expect(await page.evaluate(() => window.subscribeCalls)).toBe(1);
});

test('late subscription after timeout waits for explicit retry before saving', async ({
  page,
}) => {
  let posts = 0;
  page.on('request', (request) => {
    if (request.method() === 'POST') posts++;
  });
  await fixture(page, 'pending');
  await page.clock.install();
  await page.locator('[data-push-enable]').click();
  await page.clock.fastForward(31000);
  await expect(page.locator('[data-push-status]')).toContainText('did not respond');
  await page.evaluate(async () => {
    window.finishSubscription();
    await Promise.resolve();
  });
  await expect(page.locator('[data-push-status]')).toContainText('did not respond');
  expect(posts).toBe(0);
  await page.locator('[data-push-enable]').click();
  await expect(page.locator('[data-push-status]')).toHaveText(
    'Notifications are enabled in this browser.',
  );
  expect(posts).toBe(1);
});

test('enable after disabling creates a fresh browser subscription', async ({ page }) => {
  await fixture(page, 'success');
  await page.locator('[data-push-enable]').click();
  await expect(page.locator('[data-push-status]')).toHaveText(
    'Notifications are enabled in this browser.',
  );
  await page.locator('[data-push-disable]').click();
  await expect(page.locator('[data-push-status]')).toHaveText(
    'Notifications are off in this browser.',
  );
  await page.locator('[data-push-enable]').click();
  await expect(page.locator('[data-push-status]')).toHaveText(
    'Notifications are enabled in this browser.',
  );
  expect(await page.evaluate(() => window.subscribeCalls)).toBe(2);
});
