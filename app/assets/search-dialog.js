import { navigate } from './page-navigation.js';
import { createScope } from './page-lifecycle.js';
import { beginLoading } from './loading.js';
import { outsideDialog } from './dialog.js';

function destination(hit, overview) {
  const id = encodeURIComponent(hit.task_id || hit.id);
  if (hit.task_id || hit.kind === 'task') return `/tasks/${id}`;
  if (hit.kind === 'workspace') return `/workspaces/${id}`;
  if (hit.kind === 'list') return `/lists/${id}`;
  if (hit.kind === 'section')
    return `/lists/${encodeURIComponent(hit.list_id)}?section=${id}`;
  if (hit.kind === 'tag') return `/search?tag=${id}`;
  return overview;
}

const introduction = 'Search tasks, spaces, lists, comments, tags and files.';
export function searchOmnibar() {
  const scope = createScope();
  let dialog;
  let controller;
  let timer;
  let skeleton;
  let generation = 0;
  let outsidePress = false;
  function cancel() {
    clearTimeout(timer);
    controller?.abort();
    generation++;
    skeleton?.finish();
  }
  return {
    query: '',
    hits: [],
    active: -1,
    busy: false,
    copied: null,
    status: introduction,
    icon: 'info',
    init() {
      dialog = this.$el;
    },
    destroy() {
      cancel();
      scope.dispose();
    },
    get statusIcon() {
      return `/assets/icons.svg#${this.icon}`;
    },
    get overviewHref() {
      return `/search?query=${encodeURIComponent(this.query.trim())}&status=all`;
    },
    get selectedId() {
      return this.hits[this.active]?.rowId || null;
    },
    open() {
      if (!dialog.open) dialog.showModal();
      this.$refs.input.focus();
      this.$refs.input.select();
      this.search();
    },
    close() {
      dialog.close();
    },
    closed() {
      cancel();
      this.query = '';
      this.hits = [];
      this.active = -1;
      this.busy = false;
      this.status = introduction;
      this.icon = 'info';
    },
    openFromTrigger(event) {
      if (
        event.button !== 0 ||
        event.metaKey ||
        event.ctrlKey ||
        event.shiftKey ||
        event.altKey
      )
        return;
      if (
        !event.target.closest('[data-search-open],[data-open-search],a[href="/search"]')
      )
        return;
      event.preventDefault();
      this.open();
    },
    shortcut(event) {
      if (
        event.defaultPrevented ||
        event.isComposing ||
        event.altKey ||
        document.querySelector('.confirmation-dialog[open]')
      )
        return;
      const editing = event.target.closest(
        'input,textarea,select,[contenteditable="true"]',
      );
      if (
        ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') ||
        (event.key === '/' && !editing && !event.metaKey && !event.ctrlKey)
      ) {
        event.preventDefault();
        event.stopPropagation();
        this.open();
      }
    },
    backdropPress(event) {
      outsidePress = outsideDialog(dialog, event);
    },
    backdropClick(event) {
      if (outsidePress && outsideDialog(dialog, event)) this.close();
      outsidePress = false;
    },
    changed() {
      cancel();
      this.hits = [];
      this.active = -1;
      this.busy = Boolean(this.query.trim());
      this.status = introduction;
      this.icon = 'info';
      if (this.busy) skeleton = beginLoading(this.$refs.results);
      timer = setTimeout(() => this.search(), 220);
    },
    select(index) {
      this.active = this.hits.length ? (index + this.hits.length) % this.hits.length : -1;
      this.$nextTick(() => {
        if (this.selectedId)
          document.getElementById(this.selectedId)?.scrollIntoView({ block: 'nearest' });
      });
    },
    hover(event) {
      this.select(Number(event.currentTarget.dataset.index));
    },
    searchKey(event) {
      if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
        event.preventDefault();
        this.select(this.active + (event.key === 'ArrowDown' ? 1 : -1));
      } else if (event.key === 'Enter') {
        event.preventDefault();
        const href = this.hits[this.active]?.href || this.overviewHref;
        this.close();
        navigate(href);
      }
    },
    async search() {
      cancel();
      const current = generation;
      const text = this.query.trim();
      this.hits = [];
      this.active = -1;
      if (!text) {
        this.busy = false;
        this.status = introduction;
        return;
      }
      this.busy = true;
      const loading = (skeleton = beginLoading(this.$refs.results));
      controller = new AbortController();
      try {
        const response = await scope.request(
          `/api/v1/query?text=${encodeURIComponent(text)}&status=all&limit=8&sort=relevance`,
          { signal: controller.signal, cache: 'no-store' },
        );
        if (!response.ok)
          throw Error(
            response.status === 401
              ? 'Your session ended. Sign in again.'
              : 'Search is unavailable. Try again.',
          );
        const data = await response.json();
        if (current !== generation || !dialog.open || scope.signal.aborted) return;
        this.hits = data.hits.map((hit, index) => ({
          ...hit,
          title: hit.title || hit.kind,
          rowId: `omnibar-result-${index}`,
          href: destination(hit, this.overviewHref),
          contextLabel: [
            hit.kind,
            hit.complete ? 'completed' : '',
            ...(hit.context || []).map((item) => item.title),
          ]
            .filter(Boolean)
            .join(' · '),
          copyLabel: `Copy reference ${hit.reference}`,
        }));
        this.status = data.hits.length
          ? `${data.total} matching ${data.total === 1 ? 'entry' : 'entries'}. Showing the most relevant.`
          : 'No matches. Try a different word or a shorter search.';
        this.icon = data.hits.length ? 'info' : 'search';
        this.select(0);
      } catch (error) {
        if (
          current === generation &&
          !scope.signal.aborted &&
          error.name !== 'AbortError'
        ) {
          this.status = error.message;
          this.icon = 'info';
        }
      } finally {
        loading.finish();
        if (current === generation) this.busy = false;
      }
    },
    async copyReference(event) {
      const hit = this.hits[Number(event.currentTarget.dataset.index)];
      try {
        await navigator.clipboard.writeText(hit.reference);
        if (scope.signal.aborted) return;
        this.status = `Copied ${hit.reference}`;
        this.icon = 'check';
        this.copied = hit.rowId;
        scope.timeout(() => {
          this.copied = null;
        }, 1600);
      } catch {
        this.status = `Could not copy to clipboard. Reference: ${hit.reference}`;
        this.icon = 'info';
      }
    },
  };
}
