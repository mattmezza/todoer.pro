#!/usr/bin/env python3
"""Stage only public static assets, never generator code or internal sources."""
from pathlib import Path
import shutil

root = Path(__file__).resolve().parent.parent / 'www'
out = root / '_site'
if out.exists():
    shutil.rmtree(out)
out.mkdir()
allowed = {'.html', '.css', '.js', '.svg', '.png', '.jpg', '.jpeg', '.webp', '.ico', '.woff2'}
for source in root.rglob('*'):
    relative = source.relative_to(root)
    if any(part in {'_site', 'tools', 'content', 'node_modules'} for part in relative.parts):
        continue
    if not source.is_file():
        continue
    if source.suffix not in allowed and str(relative) not in {'_headers', '_redirects', 'robots.txt', 'llms.txt', 'sitemap.xml', 'pricing.json', 'pricing.md'}:
        continue
    destination = out / relative
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
print(f'Staged public website in {out}')
