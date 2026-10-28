import { beginLoading } from './loading.js';
import { request } from './request.js';
import { welcomeChase } from './welcome-chase.js';
const root = document.querySelector('[data-welcome]');
const setup = root.querySelector('[data-welcome-setup]');
const listsForm = root.querySelector('[data-welcome-lists]');
const error = root.querySelector('[data-welcome-error]');
const reload = root.querySelector('[data-welcome-reload]');
const heading = root.querySelector('h1');
const csrf = document.querySelector('meta[name="csrf-token"]').content;
const chase = welcomeChase();
let state,
  busy = false;
const selectedLists = new Map();
const customNames = new Map();
const suggestions = {
  personal: [
    ['weekly-tasks', 'Weekly tasks', 'The little things to get done.', 'blue'],
    ['groceries', 'Groceries', 'One place for the next shop.', 'green'],
    ['home-chores', 'Home chores', 'Keep home running smoothly.', 'amber'],
    ['personal-goals', 'Personal goals', 'Make room for what matters to you.', 'pink'],
  ],
  work: [
    [
      'weekly-priorities',
      'Weekly priorities',
      'Keep your next steps in sight.',
      'violet',
    ],
    ['project', 'A project', 'Turn a bigger goal into smaller tasks.', 'violet'],
    ['follow-ups', 'Follow-ups', 'Keep loose ends from slipping away.', 'amber'],
    ['ideas', 'Ideas', 'Capture a thought before it goes.', 'green'],
  ],
};
const spaces = () =>
  [...setup.querySelectorAll('input:checked')].map((input) => input.value);
const slug = (value) =>
  [
    ...value
      .toLowerCase()
      .replace(/[^\p{L}\p{N}]+/gu, '-')
      .replace(/^-+|-+$/g, ''),
  ]
    .slice(0, 64)
    .join('')
    .replace(/-+$/, '');
function choices() {
  const result = [];
  for (const space of spaces()) {
    for (const title of selectedLists.get(space) || []) result.push({ space, title });
    const title = slug(customNames.get(space) || '');
    if (
      title &&
      !result.some((choice) => choice.space === space && choice.title === title)
    )
      result.push({ space, title });
  }
  return result;
}
function updateButtons() {
  setup.querySelector('[type=submit]').disabled = busy || !spaces().length;
  listsForm.querySelector('[type=submit]').disabled = busy || !choices().length;
}
function listGroups() {
  const groups = root.querySelector('[data-welcome-list-groups]');
  groups.replaceChildren();
  for (const space of spaces()) {
    const group = document.createElement('fieldset');
    group.className = 'welcome-list-group';
    const legend = document.createElement('legend');
    legend.textContent = `@${space}`;
    group.append(legend);
    const cards = document.createElement('div');
    cards.className = 'welcome-choices welcome-list-choices';
    for (const [title, name, description, color] of suggestions[space]) {
      const label = document.createElement('label');
      label.className = 'welcome-choice';
      const input = document.createElement('input');
      input.type = 'checkbox';
      input.value = title;
      input.checked = selectedLists.get(space)?.has(title) || false;
      input.addEventListener('change', () => {
        if (!selectedLists.has(space)) selectedLists.set(space, new Set());
        selectedLists.get(space)[input.checked ? 'add' : 'delete'](title);
        updateButtons();
      });
      const mark = document.createElement('span');
      mark.className = 'welcome-choice-mark';
      mark.textContent = '✓';
      mark.setAttribute('aria-hidden', 'true');
      const strong = document.createElement('strong');
      const dot = document.createElement('i');
      dot.className = `welcome-list-dot swatch-${color}`;
      dot.setAttribute('aria-hidden', 'true');
      strong.append(dot, document.createTextNode(name));
      const small = document.createElement('small');
      small.textContent = description;
      const destination = document.createElement('span');
      destination.className = 'welcome-destination';
      destination.textContent = `@${space} / #${title}`;
      label.append(input, mark, strong, small, destination);
      cards.append(label);
    }
    const custom = document.createElement('details');
    custom.className = 'welcome-custom';
    const summary = document.createElement('summary');
    summary.textContent = 'Add your own…';
    const label = document.createElement('label');
    label.textContent = `List name in @${space}`;
    const input = document.createElement('input');
    input.type = 'text';
    input.maxLength = 64;
    input.autocomplete = 'off';
    input.setAttribute('autocapitalize', 'none');
    input.spellcheck = false;
    input.value = customNames.get(space) || '';
    input.addEventListener('input', () => {
      customNames.set(space, input.value);
      updateButtons();
    });
    input.addEventListener('blur', () => {
      input.value = slug(input.value);
      customNames.set(space, input.value);
      updateButtons();
    });
    if (input.value) custom.open = true;
    label.append(input);
    custom.append(summary, label);
    group.append(cards, custom);
    groups.append(group);
  }
}
function step(number, focus = true) {
  root.dataset.step = number;
  setup.hidden = number !== 1;
  listsForm.hidden = number !== 2;
  root.querySelector('[data-welcome-count]').textContent = `Step ${number} of 2`;
  root
    .querySelectorAll('.welcome-progress i')
    .forEach((dot, index) => dot.classList.toggle('active', index < number));
  heading.textContent =
    number === 1
      ? 'Where would a little more order help?'
      : 'What would you like to get organized?';
  root.querySelector('[data-welcome-intro]').textContent =
    number === 1
      ? 'A space keeps the lists for one part of your life together.'
      : 'A list brings related tasks together, so you know where things belong.';
  if (number === 2) listGroups();
  updateButtons();
  if (focus) heading.focus({ preventScroll: true });
}
async function api(path, data) {
  const response = await request('/api/v1/onboarding' + path, {
    credentials: 'same-origin',
    cache: 'no-store',
    method: data ? 'POST' : 'GET',
    headers: data ? { 'Content-Type': 'application/json', 'X-CSRF-Token': csrf } : {},
    body: data ? JSON.stringify(data) : undefined,
  });
  const result = await response.json();
  if (!response.ok)
    throw Error(result.error?.message || 'Could not save your welcome setup.');
  return result;
}
async function run(action) {
  if (busy) return;
  busy = true;
  error.hidden = true;
  const controls = [...root.querySelectorAll('input,button')];
  const disabled = controls.map((el) => el.disabled);
  controls.forEach((el) => (el.disabled = true));
  root.setAttribute('aria-busy', 'true');
  try {
    await action();
  } catch (e) {
    error.textContent = e.message;
    error.hidden = false;
    reload.hidden = false;
    root.querySelector('[data-welcome-exit]').hidden = false;
  } finally {
    busy = false;
    controls.forEach((el, i) => (el.disabled = disabled[i]));
    root.removeAttribute('aria-busy');
    updateButtons();
  }
}
function apply(data) {
  state = data;
  setup
    .querySelectorAll('input')
    .forEach(
      (input) =>
        (input.checked = data.status === 'pending' && data.spaces.includes(input.value)),
    );
  step(data.status === 'pending' && data.step === 1 && data.spaces.length ? 2 : 1, false);
  root.querySelector('[data-welcome-loading]').hidden = true;
}
async function leave(result, celebrate = false) {
  const next = root.dataset.next;
  const requested = next && !['/', '/inbox', '/today'].includes(next.split('?')[0]);
  const destination = requested ? next : result.redirect;
  if (celebrate) {
    root.dataset.ready = 'true';
    root.querySelector('.welcome-content').setAttribute('inert', '');
    root.querySelector('[data-welcome-complete]').hidden = false;
    await chase.finish();
  }
  location.assign(destination);
}
setup.addEventListener('change', updateButtons);
setup.addEventListener('submit', (event) => {
  event.preventDefault();
  const selected = spaces();
  run(async () => {
    state = await api('/setup', { spaces: selected, revision: state.revision });
    step(2);
  });
});
function finish(later = false) {
  const lists = later ? [] : choices();
  run(async () => leave(await api('/finish', { revision: state.revision, lists }), true));
}
listsForm.addEventListener('submit', (event) => {
  event.preventDefault();
  finish();
});
root.querySelector('[data-welcome-later]').addEventListener('click', () => finish(true));
root.querySelector('[data-welcome-back]').addEventListener('click', () => step(1));
async function load() {
  reload.hidden = true;
  const skeleton = beginLoading(root.querySelector('[data-welcome-loading]'), { variant: 'choices', preserve: Boolean(state) });
  const wasPending = state?.status === 'pending';
  await run(async () => {
    const data = await api('');
    if (wasPending && data.status !== 'pending')
      await leave({ redirect: data.destination }, true);
    else apply(data);
  });
  skeleton.finish();
  if (!state) {
    reload.hidden = false;
    root.querySelector('[data-welcome-loading]').hidden = true;
  }
}
reload.addEventListener('click', load);
load();
