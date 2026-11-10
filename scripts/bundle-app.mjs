import { createHash } from 'node:crypto';
// Named controller entries preserve navigation's module contract. Shared imports
// are emitted once; dynamic imports keep settings/planning code off cold pages.
import { build } from 'esbuild';
import { mkdir, rm, writeFile, readFile } from 'node:fs/promises';
const imageUrls = new Map();
await rm('app/assets/images', { recursive: true, force: true });
await mkdir('app/assets/images', { recursive: true });
for (const filename of ['icons.svg', 'icon192-v2.png']) {
  const content = await readFile(`app/assets/${filename}`);
  const hash = createHash('sha256').update(content).digest('hex').slice(0, 16);
  const dot = filename.lastIndexOf('.');
  const name = `${filename.slice(0, dot)}-${hash}${filename.slice(dot)}`;
  await writeFile(`app/assets/images/${name}`, content);
  imageUrls.set(`/assets/${filename}`, `/assets/images/${name}`);
}
const entries = [
  'site',
  'app',
  'task-page',
  'planning-page',
  'planning-workbench',
  'general-settings',
  'space-overview',
  'workspaces',
  'notification-picker',
  'capture-preferences',
  'account-security',
  'account-auth',
  'ai-billing',
  'push',
  'review',
  'admin-email',
  'delivery-settings',
  'welcome',
  'page-navigation',
  'quick-capture',
  'loading',
  'task-panel',
  'smart-capture',
  'task-list-refresh',
  'confirm',
  'request',
  'list-settings',
  'search-omnibar',
  'export-menu',
  'structured-edit',
];
const outdir = 'app/assets/bundles';
await rm(outdir, { recursive: true, force: true });
await mkdir(outdir, { recursive: true });
const { metafile } = await build({
  entryPoints: entries.map((name) => `app/assets/${name}.js`),
  outdir,
  bundle: true,
  splitting: true,
  format: 'esm',
  target: 'es2022',
  minify: true,
  legalComments: 'eof',
  metafile: true,
  plugins: [
    {
      name: 'fingerprinted-images',
      setup(build) {
        build.onLoad({ filter: /\.m?js$/ }, async ({ path }) => {
          let contents = await readFile(path, 'utf8');
          for (const [source, url] of imageUrls)
            contents = contents.replaceAll(source, url);
          return { contents, loader: 'js' };
        });
      },
    },
  ],
});
await writeFile(
  `${outdir}/entries.txt`,
  entries
    .sort()
    .map((name) => `${name}.js`)
    .join(','),
);
// Catch growth in cold-page dependency fan-out during every asset build.
const seen = new Set();
function visit(path) {
  if (seen.has(path)) return;
  seen.add(path);
  for (const item of metafile.outputs[path].imports)
    if (item.kind === 'import-statement') visit(item.path);
}
visit(`${outdir}/site.js`);
visit(`${outdir}/space-overview.js`);
if (seen.size > 24)
  throw new Error(`Space JavaScript request budget exceeded: ${seen.size}`);
console.log(`Space JavaScript: ${seen.size} files, shared and minified`);
