import { refreshPage } from './page-navigation.js';
import { beginLoading } from './loading.js';
import { toast } from './toast.js';
import { setSaveState } from './quiet-info.js';
import { createScope } from './page-lifecycle.js';

// Both preference forms update the same document. Read its latest version within
// the queue so simultaneous changes cannot overwrite one another's fields.
let preferenceWrite = Promise.resolve();
async function preferences(scope) {
  const response = await scope.request('/api/v1/notification-preferences', {
    cache: 'no-store',
  });
  if (!response.ok) throw Error('Could not load preferences.');
  return response.json();
}
function savePreferences(scope, choice) {
  const next = preferenceWrite.then(async () => {
    const value = { ...(await preferences(scope)), ...choice };
    const response = await scope.request('/api/v1/notification-preferences', {
      method: 'PUT',
      headers: {
        'Content-Type': 'application/json',
        'X-CSRF-Token': document.querySelector('meta[name=csrf-token]').content,
      },
      body: JSON.stringify(value),
    });
    if (!response.ok)
      throw Error(
        (await response.json()).error?.message || 'Could not save preferences.',
      );
    return value;
  });
  preferenceWrite = next.catch(() => {});
  return next;
}

export function calendarPreferences() {
  const scope = createScope();
  return {
    loaded: false,
    busy: false,
    timezone: '',
    weekStart: '1',
    timezones: ['UTC', ...(Intl.supportedValuesOf?.('timeZone') || [])],
    async init() {
      const skeleton = beginLoading(this.$el, { variant: 'form' });
      scope.cleanup(() => skeleton.finish());
      try {
        const value = await preferences(scope);
        if (scope.signal.aborted) return;
        this.timezone = value.timezone;
        this.weekStart = String(value.week_start ?? 1);
        this.loaded = true;
      } catch (error) {
        if (scope.signal.aborted) return;
        skeleton.fail(error.message, () => refreshPage());
        setSaveState('preferences', 'error', error.message);
      } finally {
        skeleton.finish();
      }
    },
    destroy() {
      scope.dispose();
    },
    async save() {
      if (!this.loaded || this.busy) return;
      this.busy = true;
      setSaveState('preferences', 'saving');
      try {
        await savePreferences(scope, {
          timezone: this.timezone.trim(),
          week_start: Number(this.weekStart),
        });
        if (scope.signal.aborted) return;
        setSaveState('preferences', 'saved');
        toast('Preferences saved.');
      } catch (error) {
        if (!scope.signal.aborted) setSaveState('preferences', 'error', error.message);
      } finally {
        this.busy = false;
      }
    },
  };
}

export function playPreferences() {
  const scope = createScope();
  return {
    loaded: false,
    busy: false,
    enabled: false,
    animation: 'tasks',
    async init() {
      const skeleton = beginLoading(this.$el, { variant: 'form' });
      scope.cleanup(() => skeleton.finish());
      try {
        const value = await preferences(scope);
        if (scope.signal.aborted) return;
        this.enabled = value.fun_animations !== false;
        this.animation = value.fun_animation || 'tasks';
        this.loaded = true;
      } catch (error) {
        if (scope.signal.aborted) return;
        skeleton.fail(error.message, () => refreshPage());
        setSaveState('preferences', 'error', error.message);
      } finally {
        skeleton.finish();
      }
    },
    destroy() {
      scope.dispose();
    },
    async save() {
      if (!this.loaded || this.busy) return;
      this.busy = true;
      setSaveState('preferences', 'saving');
      try {
        const value = await savePreferences(scope, {
          fun_animations: this.enabled,
          fun_animation: this.animation,
        });
        if (scope.signal.aborted) return;
        document.dispatchEvent(
          new CustomEvent('todoer-play-preferences', { detail: value }),
        );
        setSaveState('preferences', 'saved');
        toast('Preferences saved.');
      } catch (error) {
        if (!scope.signal.aborted) setSaveState('preferences', 'error', error.message);
      } finally {
        this.busy = false;
      }
    },
  };
}
