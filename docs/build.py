#!/usr/bin/env python3
"""Build the public documentation, with no network requests or executable Markdown."""
from __future__ import annotations

import html
import json
import re
import shutil
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import quote, unquote, urlsplit

from markdown_it import MarkdownIt

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parent
OUT = ROOT / '_site'
ORIGIN = 'https://docs.todoer.pro'
SOURCE = 'https://github.com/mattmezza/todoer.pro/blob/main/'

# Explicit publication list: planning notes and internal acceptance ledgers stay source-only.
GROUPS = [
    ('Start here', [
        ('getting-started', 'Getting started'),
        ('rebuild/onboarding', 'Your first spaces'),
        ('rebuild/smart-capture', 'Capture tasks'),
        ('rebuild/short-ids-defaults', 'Lists and short links'),
    ]),
    ('Your everyday workflow', [
        ('rebuild/references', 'Organize your work'),
        ('rebuild/scheduling', 'Dates and recurring tasks'),
        ('rebuild/search', 'Search'),
        ('rebuild/statistics', 'Weekly review'),
        ('rebuild/attachments', 'Comments and files'),
        ('rebuild/sharing-moves', 'Sharing and list moves'),
        ('rebuild/section-moves', 'Move sections'),
        ('rebuild/notification-center', 'Notification center'),
        ('rebuild/notifications', 'Notification settings'),
        ('rebuild/push', 'Browser reminders'),
        ('rebuild/pwa', 'Install and use offline'),
        ('rebuild/jobs', 'Background jobs'),
        ('rebuild/loading-states', 'Loading and recovery'),
        ('rebuild/slugs', 'Readable URLs'),
    ]),
    ('CLI, API and AI', [
        ('rebuild/cli-login', 'CLI sign-in'),
        ('rebuild/agent-workflow', 'CLI and agent workflow'),
        ('rebuild/agent-prompts', 'Prompts for your agent'),
        ('rebuild/authentication', 'Identity and permissions'),
        ('rebuild/rest', 'REST API'),
        ('rebuild/subtree-query', 'Task tree queries'),
        ('rebuild/bulk', 'Preview and apply changes'),
        ('rebuild/assist', 'Optional AI assistance'),
    ]),
    ('Run your own Todoer', [
        ('rebuild/deployment', 'Deploy with Docker'),
        ('rebuild/migration-export', 'Migration and export'),
        ('rebuild/local-recovery', 'Back up and restore'),
        ('rebuild/replication', 'Database replication'),
        ('rebuild/ci', 'Builds and releases'),
    ]),
]
PAGES = {key: (title, group) for group, entries in GROUPS for key, title in entries}


def esc(value):
    return html.escape(str(value), quote=True)


def url(key):
    return '/' if key == 'index' else '/' + key + '.html'


def plain(tokens):
    return ''.join(token.content for token in tokens if token.type in ('text', 'code_inline'))


def rewrite_link(value, key):
    parts = urlsplit(value)
    if parts.scheme or parts.netloc or not parts.path:
        return value
    if parts.path.startswith('/'):
        return value
    path = (ROOT / (key + '.md')).parent / unquote(parts.path)
    path = path.resolve()
    try:
        relative = path.relative_to(ROOT)
    except ValueError:
        try:
            return SOURCE + quote(str(path.relative_to(REPO))) + ('#' + parts.fragment if parts.fragment else '')
        except ValueError:
            raise ValueError(f'Link outside repository: {key}: {value}') from None
    target = str(relative.with_suffix(''))
    suffix = ('?' + parts.query if parts.query else '') + ('#' + parts.fragment if parts.fragment else '')
    if relative.suffix == '.md' and target in PAGES:
        return url(target) + suffix
    # Source-only documentation and repository resources remain browsable on GitHub.
    return SOURCE + 'docs/' + quote(str(relative)) + suffix


def render(key, source):
    md = MarkdownIt('commonmark', {'html': False, 'typographer': False}).enable('table')
    tokens = md.parse(source)
    # Template owns the single page title.
    if tokens and tokens[0].type == 'heading_open' and tokens[0].tag == 'h1':
        tokens = tokens[3:]
    headings, ids, text = [], {}, []
    for i, token in enumerate(tokens):
        if token.type == 'heading_open':
            label = plain(tokens[i + 1].children or [])
            slug = re.sub(r'[^\w\- ]', '', label.lower()).strip().replace(' ', '-') or 'section'
            n = ids.get(slug, 0)
            ids[slug] = n + 1
            if n:
                slug += '-' + str(n)
            token.attrSet('id', slug)
            if token.tag in ('h2', 'h3'):
                headings.append((token.tag, label, slug))
        if token.type == 'inline':
            text.append(plain(token.children or []))
            for child in token.children or []:
                if child.type == 'link_open':
                    child.attrSet('href', rewrite_link(child.attrGet('href'), key))
                elif child.type == 'image':
                    child.attrSet('src', rewrite_link(child.attrGet('src'), key))
    return md.renderer.render(tokens, md.options, {}), headings, '\n'.join(text)


def machine_source(key, source):
    """Publish only allowlisted guides, with links usable outside their page context."""
    md = MarkdownIt('commonmark', {'html': False}).enable('table')
    replacements = {}
    for token in md.parse(source):
        for child in token.children or []:
            if child.type not in ('link_open', 'image'):
                continue
            value = child.attrGet('href' if child.type == 'link_open' else 'src')
            rewritten = rewrite_link(value, key)
            if rewritten.startswith('/'):
                parts = urlsplit(rewritten)
                path = parts.path.removesuffix('.html') + '.md' if parts.path.endswith('.html') else parts.path
                rewritten = ORIGIN + path + ('?' + parts.query if parts.query else '') + ('#' + parts.fragment if parts.fragment else '')
            replacements[value] = rewritten
    for value, rewritten in replacements.items():
        source = source.replace('](' + value + ')', '](' + rewritten + ')')
    return f'<!-- Canonical page: {ORIGIN}{url(key)} -->\n\n' + source.rstrip() + '\n'


def navigation(current):
    sections = []
    for group, entries in GROUPS:
        links = ''.join(
            f'<a href="{url(key)}"' + (' aria-current="page"' if key == current else '') + f'>{esc(title)}</a>'
            for key, title in entries
        )
        sections.append(f'<div class="nav-group"><p>{esc(group)}</p>{links}</div>')
    return '<a class="overview" href="/"' + (' aria-current="page"' if current == 'index' else '') + '>Documentation home <span>↗︎</span></a>' + ''.join(sections)


def shell(key, title, group, content, headings=()):
    toc = ''.join(f'<a href="#{esc(anchor)}" class="level-{tag}">{esc(label)}</a>' for tag, label, anchor in headings)
    toc_html = f'<aside class="page-toc" aria-label="On this page"><p class="eyebrow">On this page</p><nav>{toc}</nav></aside>' if toc else ''
    source = f'<a href="{SOURCE}docs/{key}.md">View source <span aria-hidden="true">↗︎</span></a>' if key != 'index' else '<a href="https://github.com/mattmezza/todoer.pro">View repository ↗︎</a>'
    markdown = f'<a href="/{key}.md">Read as Markdown</a>' if key != 'index' else '<a href="/llms.txt">Agent documentation index</a>'
    return f'''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{esc(title)} · Todoer Docs</title><meta name="description" content="{esc('Todoer documentation: ' + title + '. Guides for tasks, the CLI, integrations and self-hosting.')}">
<link rel="alternate" type="text/markdown" href="{ORIGIN}/{key + ".md" if key != "index" else "llms.txt"}"><link rel="canonical" href="{ORIGIN}{url(key)}"><meta name="theme-color" content="#030712"><link rel="icon" href="/assets/favicon.svg" type="image/svg+xml">
<link rel="stylesheet" href="/assets/docs.css"><script src="/assets/docs.js" defer></script></head>
<body><a class="skip" href="#main">Skip to content</a>
<header class="site-header"><a class="brand" href="/" aria-label="Todoer documentation home"><img src="/assets/favicon.svg" width="29" height="29" alt="">todoer<span>pro</span><i></i><b>docs</b></a>
<div class="header-actions"><button class="search-launch" type="button" data-search-open hidden><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 5 5"/></svg><span>Search documentation</span><kbd>⌘ K</kbd></button><a class="website-link" href="https://todoer.pro">Website ↗︎</a><a class="app-link" href="https://app.todoer.pro">Open Todoer <span aria-hidden="true">↗︎</span></a></div></header>
<details class="mobile-navigation"><summary>Browse documentation <span aria-hidden="true">⌄</span></summary><nav aria-label="Mobile documentation">{navigation(key)}</nav></details>
<div class="docs-layout{' home-layout' if key == 'index' else ''}"><aside class="sidebar"><nav aria-label="Documentation">{navigation(key)}</nav><div class="sidebar-footer"><span class="status-dot"></span>Open source. MIT licensed.<a href="https://github.com/mattmezza/todoer.pro">GitHub ↗︎</a></div></aside>
<main id="main" tabindex="-1"><div class="page-heading"><p class="eyebrow">{esc(group)}</p><h1>{esc(title)}</h1></div>{content}<footer class="article-footer">{source}{markdown}<span>Made for a clearer next step.</span></footer></main>{toc_html}</div>
<dialog class="search-dialog" aria-labelledby="search-title"><div class="search-top"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 5 5"/></svg><label class="sr-only" id="search-title" for="docs-search">Search documentation</label><input id="docs-search" type="search" placeholder="Search the docs…" autocomplete="off" aria-controls="search-results"><button type="button" data-search-close aria-label="Close search">Esc</button></div><div class="search-results" id="search-results" aria-live="polite"><p class="search-hint">Search tasks, CLI commands, deployment and more.</p></div><div class="search-footer">Local search. Your query stays in your browser.</div></dialog>
</body></html>'''


def home():
    cards = [
        ('01', 'A place for the next step', 'Capture the thought, make a list and give your day a little breathing room.', 'getting-started', 'Get started'),
        ('02', 'Make the workflow yours', 'Dates, sections, sharing and the context behind your tasks.', 'rebuild/references', 'Organize your work'),
        ('03', 'Bring your own tools', 'Use the CLI, REST API or a compatible AI client with explicit authorization.', 'rebuild/cli-login', 'Connect your tools'),
        ('04', 'Your server. Your Todoer.', 'Deploy with Docker, keep backups and move your existing work safely.', 'rebuild/deployment', 'Start self-hosting'),
    ]
    tiles = ''.join(f'<a class="guide-card" href="{url(key)}"><span class="card-number">{n} / GUIDE</span><h2>{title}</h2><p>{description}</p><span class="card-action">{action} <span aria-hidden="true">→</span></span></a>' for n, title, description, key, action in cards)
    return f'''<p class="home-lead">Less searching. More starting.<br>Find your way around Todoer, from your first task to your own deployment.</p><div class="home-actions"><a class="primary-button" href="/getting-started.html">Start with one task <span aria-hidden="true">→</span></a><a href="https://app.todoer.pro">Open your workspace ↗︎</a></div>
<div class="guide-grid">{tiles}</div><section class="quick-links"><div><p class="eyebrow">Pick up where you are</p><h2>A few useful shortcuts.</h2></div><div><a href="/rebuild/smart-capture.html">Capture a task <span>→</span></a><a href="/rebuild/search.html">Find what you need <span>→</span></a><a href="/rebuild/agent-workflow.html">Use the CLI <span>→</span></a><a href="/rebuild/migration-export.html">Export or migrate your work <span>→</span></a></div></section><aside class="home-note"><span aria-hidden="true">↳</span><p>The web app works on its own. AI assistance and agent connections are optional, and depend on your server’s configuration. <a href="/rebuild/assist.html">How assistance works →</a></p></aside>'''


class Links(HTMLParser):
    def __init__(self):
        super().__init__()
        self.targets = []
        self.ids = set()

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if 'id' in attrs:
            self.ids.add(attrs['id'])
        if tag == 'a' and 'href' in attrs:
            self.targets.append(attrs['href'])
        if tag in ('img', 'script') and 'src' in attrs:
            self.targets.append(attrs['src'])
        if tag == 'link' and attrs.get('rel') != 'canonical' and 'href' in attrs:
            self.targets.append(attrs['href'])


def check_links():
    parsed = {}
    for path in OUT.rglob('*.html'):
        parser = Links()
        parser.feed(path.read_text())
        parsed[path.resolve()] = parser
    for path, parser in parsed.items():
        for target in parser.targets:
            value = urlsplit(target)
            if value.scheme or value.netloc:
                continue
            destination = (OUT / value.path.lstrip('/') if value.path.startswith('/') else path.parent / value.path).resolve() if value.path else path
            if destination.is_dir():
                destination /= 'index.html'
            if not destination.is_file():
                raise ValueError(f'Broken link in {path.relative_to(OUT)}: {target}')
            if value.fragment and destination in parsed and unquote(value.fragment) not in parsed[destination].ids:
                raise ValueError(f'Broken heading link in {path.relative_to(OUT)}: {target}')


    # Markdown is a public interface too: verify links between its published guides.
    md = MarkdownIt('commonmark', {'html': False}).enable('table')
    for path in OUT.rglob('*.md'):
        for token in md.parse(path.read_text()):
            for child in token.children or []:
                if child.type != 'link_open':
                    continue
                target = child.attrGet('href')
                value = urlsplit(target)
                if value.netloc and value.netloc != urlsplit(ORIGIN).netloc:
                    continue
                if value.scheme and value.scheme not in ('http', 'https'):
                    continue
                destination = (OUT / value.path.lstrip('/') if value.path.startswith('/') else path.parent / value.path).resolve() if value.path else path
                if destination.is_dir():
                    destination /= 'index.html'
                if not destination.is_file():
                    raise ValueError(f'Broken Markdown link in {path.relative_to(OUT)}: {target}')
                html_destination = destination.with_suffix('.html') if destination.suffix == '.md' else destination
                if value.fragment and html_destination in parsed and unquote(value.fragment) not in parsed[html_destination].ids:
                    raise ValueError(f'Broken Markdown heading in {path.relative_to(OUT)}: {target}')


def main():
    if OUT.exists():
        shutil.rmtree(OUT)
    (OUT / 'assets').mkdir(parents=True)
    for asset in (ROOT / 'assets').iterdir():
        if asset.is_file():
            shutil.copyfile(asset, OUT / 'assets' / asset.name)
    index = []
    machine_pages = []
    for key, (title, group) in PAGES.items():
        source = (ROOT / (key + '.md')).read_text()
        content, headings, text = render(key, source)
        keys = list(PAGES)
        position = keys.index(key)
        pager = []
        if position:
            previous = keys[position - 1]
            pager.append(f'<a href="{url(previous)}"><span>← Previous</span><strong>{esc(PAGES[previous][0])}</strong></a>')
        if position + 1 < len(keys):
            following = keys[position + 1]
            pager.append(f'<a href="{url(following)}"><span>Next →</span><strong>{esc(PAGES[following][0])}</strong></a>')
        body = f'<article class="prose">{content}</article><nav class="page-navigation" aria-label="Previous and next guide">' + ''.join(pager) + '</nav>'
        path = OUT / (key + '.html')
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(shell(key, title, group, body, headings))
        markdown = machine_source(key, source)
        (OUT / (key + '.md')).write_text(markdown)
        machine_pages.append(markdown)
        index.append({'title': title, 'group': group, 'url': url(key), 'text': text})
    (OUT / 'index.html').write_text(shell('index', 'A clearer way forward.', 'THE TODOER HANDBOOK', home()))
    (OUT / 'search-index.json').write_text(json.dumps(index, ensure_ascii=False, separators=(',', ':')))
    (OUT / 'CNAME').write_text('docs.todoer.pro\n')
    (OUT / '.nojekyll').touch()
    (OUT / 'robots.txt').write_text(f'User-agent: *\nAllow: /\nSitemap: {ORIGIN}/sitemap.xml\n')
    (OUT / 'sitemap.xml').write_text('<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">' + ''.join(f'<url><loc>{ORIGIN}{url(key)}</loc></url>' for key in ['index', *PAGES]) + '</urlset>\n')
    agent_index = ('# Todoer documentation\n\n'
        '> Todoer is an open-source task manager with a web app, REST API, CLI and remote MCP.\n\n'
        'Use the guides below as the source of truth. Connect to an existing instance; '
        'cloud uses https://app.todoer.pro and self-hosted instances use their own HTTPS origin. '
        'CLI uses REST credentials; MCP uses separate OAuth or MCP credentials. '
        'Discover capabilities before writes and preserve revision, idempotency and access-review checks.\n\n'
        f'- [Complete documentation]({ORIGIN}/llms-full.txt): all published guides in one file\n'
        f'- [Agent skill](https://github.com/mattmezza/todoer.pro/blob/main/skills/todoer.pro/SKILL.md): connecting and operating safely\n\n'
        + '\n'.join(f'## {group}\n\n' + '\n'.join(
            f'- [{title}]({ORIGIN}/{key}.md): [human page]({ORIGIN}{url(key)})'
            for key, title in entries) + '\n' for group, entries in GROUPS))
    full = '# Todoer complete documentation\n\n' + '\n---\n\n'.join(machine_pages)
    for name in ('llms.txt', 'llm.txt'):
        (OUT / name).write_text(agent_index)
    for name in ('llms-full.txt', 'llm-full.txt'):
        (OUT / name).write_text(full)
    check_links()
    print(f'Built {len(PAGES) + 1} pages in {OUT.relative_to(REPO)}; all local links and heading anchors verified.')


if __name__ == '__main__':
    main()
