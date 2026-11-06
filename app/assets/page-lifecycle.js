import { request } from './request.js';

// Modules register once; their controllers run once for each mounted page.
const controllers = [];
let current;

export function createScope() {
  const abort = new AbortController();
  const cleanups = new Set();
  const timers = new Set();
  const intervals = new Set();
  const frames = new Set();
  const scope = {
    signal: abort.signal,
    cleanup(fn) {
      if (typeof fn === 'function') {
        if (abort.signal.aborted) fn();
        else cleanups.add(fn);
      }
      return fn;
    },
    listen(target, type, callback, options = {}) {
      if (!target || abort.signal.aborted) return;
      const settings = typeof options === 'boolean' ? { capture: options } : options;
      target.addEventListener(type, callback, {
        ...settings,
        signal: settings.signal
          ? AbortSignal.any([settings.signal, abort.signal])
          : abort.signal,
      });
    },
    track(resource) {
      scope.cleanup(() =>
        resource.disconnect ? resource.disconnect() : resource.close(),
      );
      return resource;
    },
    timeout(callback, delay, ...args) {
      const id = setTimeout(() => {
        timers.delete(id);
        if (!abort.signal.aborted) callback(...args);
      }, delay);
      timers.add(id);
      return id;
    },
    interval(callback, delay) {
      const id = setInterval(callback, delay);
      intervals.add(id);
      return id;
    },
    frame(callback) {
      const id = requestAnimationFrame((time) => {
        frames.delete(id);
        if (!abort.signal.aborted) callback(time);
      });
      frames.add(id);
      return id;
    },
    request(input, options = {}) {
      return request(input, {
        ...options,
        signal: options.signal
          ? AbortSignal.any([options.signal, abort.signal])
          : abort.signal,
      });
    },
    dispose() {
      abort.abort();
      for (const id of timers) clearTimeout(id);
      for (const id of intervals) clearInterval(id);
      for (const id of frames) cancelAnimationFrame(id);
      for (const cleanup of [...cleanups].reverse()) cleanup();
      cleanups.clear();
    },
  };
  return scope;
}

export function pageScope() {
  return (current ||= createScope());
}

function run(controller) {
  const scope = pageScope();
  if (controller.scope === scope) return;
  controller.scope = scope;
  const result = controller.mount(scope);
  if (result?.then) result.then((cleanup) => scope.cleanup(cleanup)).catch(report);
  else scope.cleanup(result);
}

function report(error) {
  if (error.name !== 'AbortError') console.error(error);
}

export function onPageLoad(mount) {
  const controller = { mount };
  controllers.push(controller);
  run(controller);
}

export function unmountPage() {
  current?.dispose();
  current = null;
}

export function mountPage() {
  for (const controller of controllers) run(controller);
  document.dispatchEvent(new Event('todoer:page-ready'));
}
