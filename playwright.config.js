import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './rebuild/browser',
  outputDir: '.local/rebuild/browser-results',
  reporter: 'list',
  use: {
    baseURL: process.env.TODOER_BROWSER_URL || 'http://127.0.0.1:8092',
    colorScheme: 'dark',
  },
  projects: [
    { name: 'chromium', use: { browserName: 'chromium' } },
    { name: 'firefox', use: { browserName: 'firefox' } },
    { name: 'webkit', use: { browserName: 'webkit' } },
  ],
});
