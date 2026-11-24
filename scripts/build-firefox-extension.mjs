import { cp, readFile, rm, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const source = fileURLToPath(new URL('../extension', import.meta.url));
const output = fileURLToPath(
  new URL('../.local/browser-extension/firefox', import.meta.url),
);
const { minimum_chrome_version, background, ...manifest } = JSON.parse(
  await readFile(`${source}/manifest.json`, 'utf8'),
);

manifest.background = { scripts: [background.service_worker], type: background.type };
manifest.browser_specific_settings = {
  gecko: {
    id: 'browser-extension@todoer.pro',
    strict_min_version: '140.0',
    data_collection_permissions: { required: ['none'] },
  },
  gecko_android: { strict_min_version: '142.0' },
};

await rm(output, { recursive: true, force: true });
await cp(source, output, {
  recursive: true,
  filter: (path) => path !== `${source}/manifest.json`,
});
await writeFile(`${output}/manifest.json`, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(output);
