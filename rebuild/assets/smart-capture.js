import { parseCapture } from './smart-capture-parser.mjs';
import { request } from './request.js';
let cached;
async function options() {
  if (!cached)
    cached = request('/api/v1/capture/options')
      .then((r) => {
        if (!r.ok) throw Error('options unavailable');
        return r.json();
      })
      .catch(() => {
        cached = null;
        return null;
      });
  return cached;
}
export function mountSmartCapture(form) {
  if (form.smartCapture) return form.smartCapture;
  const title = form.querySelector('[name=title]');
  const wrapper = document.createElement('div');
  wrapper.className = 'smart-capture-input';
  title.parentNode.insertBefore(wrapper, title);
  wrapper.append(title);
  const suggestions = document.createElement('div');
  suggestions.className = 'capture-suggestions';
  suggestions.id = `capture-suggestions-${crypto.randomUUID()}`;
  suggestions.setAttribute('role', 'listbox');
  suggestions.setAttribute('aria-label', 'Capture suggestions');
  suggestions.hidden = true;
  wrapper.append(suggestions);
  title.setAttribute('role', 'combobox');
  title.setAttribute('aria-autocomplete', 'list');
  title.setAttribute('aria-controls', suggestions.id);
  title.setAttribute('aria-expanded', 'false');
  const block = document.createElement('div');
  block.className = 'smart-capture';
  const info = document.createElement('div');
  info.className = 'capture-result';
  info.setAttribute('aria-live', 'polite');
  const controls = document.createElement('div');
  const help = document.createElement('div');
  help.className = 'capture-help';
  const helpButton = document.createElement('button');
  helpButton.type = 'button';
  helpButton.textContent = 'Shortcuts';
  const helpText = document.createElement('span');
  helpText.textContent = '@space · #list · +tag · p0–p3 · ! / !! / !!! · today';
  helpText.title =
    'You can also use !0–!3 and date phrases. Quote names containing spaces.';
  helpText.hidden = true;
  let hovering = false,
    expired = false,
    helpTimer;
  const hideHelp = () => {
    helpText.hidden = true;
    helpButton.hidden = false;
    expired = false;
  };
  help.addEventListener('pointerenter', () => {
    hovering = true;
  });
  help.addEventListener('pointerleave', () => {
    hovering = false;
    if (expired) hideHelp();
  });
  helpButton.addEventListener('click', () => {
    helpButton.hidden = true;
    helpText.hidden = false;
    expired = false;
    clearTimeout(helpTimer);
    helpTimer = setTimeout(() => {
      expired = true;
      if (!hovering) hideHelp();
    }, 30000);
  });
  help.append(helpButton, helpText);
  block.append(info, controls, help);
  wrapper.insertAdjacentElement('afterend', block);
  let data = null,
    literal = false,
    parsed = null,
    changes = {},
    choices = [],
    active = 0,
    completion = null;
  const quote = (value) => (/\s/.test(value) ? `"${value}"` : value);
  function choose(index) {
    if (!completion || !choices[index]) return;
    const { end, start } = completion;
    const inserted = quote(choices[index]);
    title.value = title.value.slice(0, start) + inserted + title.value.slice(end);
    title.focus();
    title.setSelectionRange(start + inserted.length, start + inserted.length);
    changes = {};
    choices = [];
    completion = null;
    active = 0;
    setup();
    suggestions.hidden = true;
    title.setAttribute('aria-expanded', 'false');
    title.removeAttribute('aria-activedescendant');
  }
  function renderPreview(v) {
    info.replaceChildren();
    if (v.title.trim()) {
      const caption = document.createElement('span');
      caption.textContent = 'Will add';
      const text = document.createElement('strong');
      text.textContent = v.title.trim();
      const target = document.createElement('span');
      const fallback = form.querySelector('[name=list_id]');
      const defaultName =
        fallback?.selectedOptions?.[0]?.textContent?.trim() ||
        document
          .querySelector('.planning-list-title, [data-list-title]')
          ?.textContent?.trim() ||
        '';
      const fallbackId = fallback?.value || document.body.dataset.list;
      const fallbackDestination = data?.destinations.find(
        (d) => d.list_id === fallbackId,
      );
      const destination = v.destination || fallbackDestination;
      const location = destination
        ? `@${destination.space} / #${destination.list}`
        : defaultName || 'Current list';
      target.textContent = `To ${location}`;
      info.append(caption, text, target);
      const metadata = [
        v.due && `Due ${v.due}`,
        v.priority !== null && `P${v.priority}`,
        ...v.tags.map((t) => `+${t}`),
      ].filter(Boolean);
      if (metadata.length) {
        const details = document.createElement('span');
        details.textContent = metadata.join(' · ');
        info.append(details);
      }
    }
    if (parsed.spans.length || literal) {
      const toggle = document.createElement('button');
      toggle.type = 'button';
      toggle.className = 'capture-literal';
      toggle.textContent = literal ? 'Interpret shortcuts' : 'Use text as typed';
      toggle.title = literal
        ? 'Recognize capture shortcuts again'
        : 'Keep every word and shortcut in the task title';
      toggle.onclick = () => {
        literal = !literal;
        changes = {};
        setup();
      };
      info.append(toggle);
    }
  }
  const setup = () => {
    parsed =
      data?.preferences.smart_enabled && !literal
        ? parseCapture(title.value, {
            destinations: data.destinations,
            dates: data.preferences.date_enabled,
          })
        : {
            title: title.value,
            original: title.value,
            tags: [],
            priority: null,
            due: null,
            destination: null,
            spans: [],
            warnings: [],
          };
    const v = { ...parsed, ...changes };
    renderPreview(v);
    controls.replaceChildren();
    if (parsed.due) {
      const label = document.createElement('label');
      label.textContent = 'Due';
      const date = document.createElement('input');
      date.type = 'date';
      date.value = v.due || '';
      date.onchange = () => {
        changes.due = date.value || null;
        renderPreview({ ...parsed, ...changes });
      };
      label.append(date);
      controls.append(label);
    }
    suggestions.replaceChildren();
    choices = [];
    completion = null;
    suggestions.hidden = true;
    title.setAttribute('aria-expanded', 'false');
    title.removeAttribute('aria-activedescendant');
    if (!data || literal || !data.preferences.smart_enabled) return;
    const before = title.value.slice(0, title.selectionStart);
    const match = /(?:^|\s|(?<=@[\p{L}\p{N}_-]+))([@#+])([^\s@#+]*)$/u.exec(before);
    if (!match) return;
    const symbol = match[1],
      needle = match[2].replace(/^"/, '').toLocaleLowerCase();
    const space = /@(?:"([^"]+)"|([\p{L}\p{N}_-]+))/u.exec(before);
    let values =
      symbol === '@'
        ? data.destinations.map((d) => d.space)
        : symbol === '#'
          ? data.destinations
              .filter(
                (d) =>
                  !space ||
                  d.space.toLocaleLowerCase() ===
                    (space[1] || space[2]).toLocaleLowerCase(),
              )
              .map((d) => d.list)
          : data.tags || [];
    choices = [...new Set(values)]
      .filter((x) => x.toLocaleLowerCase().startsWith(needle))
      .slice(0, 50);
    if (!choices.length) {
      if (parsed.warnings.length) {
        const warning = document.createElement('span');
        warning.textContent = 'Unrecognized destination will stay in the title.';
        info.append(warning);
      }
      return;
    }
    active = Math.min(active, choices.length - 1);
    completion = {
      end: title.selectionStart,
      start: title.selectionStart - match[2].length,
    };
    choices.forEach((value, index) => {
      const option = document.createElement('div');
      option.setAttribute('role', 'option');
      option.id = `${suggestions.id}-${index}`;
      option.textContent = symbol + value;
      option.setAttribute('aria-selected', String(index === active));
      option.addEventListener('pointerdown', (event) => {
        event.preventDefault();
        choose(index);
      });
      suggestions.append(option);
    });
    suggestions.hidden = false;
    title.setAttribute('aria-expanded', 'true');
    title.setAttribute('aria-activedescendant', `${suggestions.id}-${active}`);
  };
  title.addEventListener('keydown', (event) => {
    if (!completion || suggestions.hidden) return;
    const down =
      event.key === 'ArrowDown' || (event.ctrlKey && event.key.toLowerCase() === 'n');
    const up =
      event.key === 'ArrowUp' || (event.ctrlKey && event.key.toLowerCase() === 'p');
    if (down || up) {
      event.preventDefault();
      event.stopPropagation();
      active = (active + (down ? 1 : -1) + choices.length) % choices.length;
      [...suggestions.children].forEach((option, index) =>
        option.setAttribute('aria-selected', String(index === active)),
      );
      const selected = suggestions.children[active];
      title.setAttribute('aria-activedescendant', selected.id);
      selected.scrollIntoView({ block: 'nearest' });
    } else if ((event.key === 'Tab' && !event.shiftKey) || event.key === 'Enter') {
      event.preventDefault();
      event.stopPropagation();
      choose(active);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      suggestions.hidden = true;
      completion = null;
      title.setAttribute('aria-expanded', 'false');
      title.removeAttribute('aria-activedescendant');
    }
  });
  title.addEventListener('input', () => {
    changes = {};
    literal = false;
    active = 0;
    setup();
  });
  title.addEventListener('click', setup);
  form.querySelector('[name=list_id]')?.addEventListener('change', setup);
  const ready = options().then((value) => {
    data = value;
    setup();
  });
  setup();
  const api = {
    help,
    ready,
    get() {
      const v = { ...parsed, ...changes };
      return {
        title: v.title.trim(),
        list_id: v.destination?.list_id,
        metadata: v.spans.length
          ? { priority: v.priority, due_date: v.due, tags: v.tags }
          : null,
        repeat: !!data?.preferences.repeat_capture,
      };
    },
    reset() {
      literal = false;
      changes = {};
      setup();
    },
  };
  form.smartCapture = api;
  return api;
}
