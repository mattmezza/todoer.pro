import { request } from './request.js';
const root = document.querySelector('[data-welcome]');
const setup = root.querySelector('[data-welcome-setup]');
const listForm = root.querySelector('[data-welcome-list]');
const error = root.querySelector('[data-welcome-error]');
const reload = root.querySelector('[data-welcome-reload]');
const heading = root.querySelector('h1');
const csrf = document.querySelector('meta[name="csrf-token"]').content;
let state,
  busy = false;
const palette = ['blue', 'violet', 'green', 'amber', 'pink'];
function preview() {
  root.querySelector('[data-preview-space]').textContent =
    slug(setup.elements.space.value) || 'your-space';
  root.querySelector('[data-preview-list]').textContent =
    slug(listForm.elements.list.value) || 'your-list';
  const color = listForm.elements.color.value || 'blue';
  const swatch = root.querySelector('[data-preview-color]');
  swatch.style.backgroundColor =
    color === 'red'
      ? '#f87171'
      : color === 'gray'
        ? '#94a3b8'
        : /^#[a-f0-9]{6}$/i.test(color)
          ? color
          : '';
  root.querySelector('[data-preview-color]').className =
    `welcome-list-dot swatch-${palette.includes(color) ? color : 'blue'}`;
}
function step(number, focus = true) {
  root.dataset.step = number;
  setup.hidden = number !== 1;
  listForm.hidden = number !== 2;
  root.querySelector('[data-welcome-count]').textContent = `Step ${number} of 2`;
  root
    .querySelectorAll('.welcome-progress i')
    .forEach((dot, index) => dot.classList.toggle('active', index < number));
  heading.textContent =
    number === 1 ? 'A space for your life.' : 'A list to bring it together.';
  root.querySelector('[data-welcome-intro]').textContent =
    number === 1
      ? 'A space groups your lists around one area of life — personal, work, or something shared.'
      : 'Lists hold related tasks inside your space. Start with one; add your tasks when you’re ready.';
  root.querySelector('[data-preview-teaching]').textContent =
    number === 1
      ? 'One space for each part of life. Private until you choose to share.'
      : 'Space → list → tasks. A simple home for whatever needs doing.';
  root.querySelector('[data-preview-list-card]').hidden = number !== 2;
  if (focus) heading.focus({ preventScroll: true });
  preview();
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
  if (!response.ok) {
    const e = Error(result.error?.message || 'Could not save your welcome setup.');
    e.conflict = response.status === 409;
    throw e;
  }
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
  }
}
function apply(data) {
  state = data;
  setup.elements.space.value = slug(data.space);
  listForm.elements.list.value = slug(data.list);
  if (
    !palette.includes(data.color) &&
    (/^#[a-f0-9]{6}$/i.test(data.color) || ['red', 'gray'].includes(data.color))
  ) {
    listForm.querySelectorAll('[data-custom-color]').forEach((el) => el.remove());
    const label = document.createElement('label');
    label.dataset.customColor = '';
    const input = document.createElement('input');
    input.type = 'radio';
    input.name = 'color';
    input.value = data.color;
    const swatch = document.createElement('span');
    swatch.className = 'welcome-swatch';
    swatch.style.backgroundColor =
      data.color === 'red' ? '#f87171' : data.color === 'gray' ? '#94a3b8' : data.color;
    swatch.textContent = 'Current color';
    label.append(input, swatch);
    listForm.querySelector('fieldset').append(label);
  }
  listForm.elements.color.value = data.color || 'blue';
  step(data.status === 'pending' && data.step === 1 ? 2 : 1, false);
  root.querySelector('[data-welcome-loading]').hidden = true;
  root.querySelector('[data-welcome-skip]').disabled = false;
}
function leave(result, celebrate = false) {
  const next = root.dataset.next;
  const requested = next && !['/', '/inbox', '/today'].includes(next.split('?')[0]);
  const destination = requested ? next : result.redirect;
  if (celebrate) {
    root.dataset.ready = 'true';
    heading.textContent = 'Your space is ready.';
    listForm.hidden = true;
    const status = root.querySelector('[data-welcome-loading]');
    status.textContent = 'A little less keeping track. A little more room for you.';
    status.hidden = false;
    setTimeout(
      () => location.assign(destination),
      matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : 450,
    );
  } else location.assign(destination);
}
setup.addEventListener('input', preview);
listForm.addEventListener('input', preview);
listForm.addEventListener('change', preview);
function slug(value) {
  return value
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 64)
    .replace(/-+$/, '');
}
for (const input of [setup.elements.space, listForm.elements.list]) {
  input.addEventListener('input', () => {
    // Keep a trailing separator while typing so multiword names stay natural.
    const normalize = (value) =>
      [
        ...value
          .toLowerCase()
          .replace(/[^\p{L}\p{N}]+/gu, '-')
          .replace(/^-+/, ''),
      ]
        .slice(0, 64)
        .join('');
    const caret = normalize(input.value.slice(0, input.selectionStart)).length;
    input.value = normalize(input.value);
    input.setSelectionRange(caret, caret);
    preview();
  });
  input.addEventListener('blur', () => {
    input.value = slug(input.value);
    preview();
  });
}
root.querySelectorAll('[data-example]').forEach((button) =>
  button.addEventListener('click', () => {
    const examples = {
      personal: ['personal', 'inbox', 'blue'],
      work: ['work', 'priorities', 'violet'],
      together: ['home', 'chores', 'green'],
    };
    [
      setup.elements.space.value,
      listForm.elements.list.value,
      listForm.elements.color.value,
    ] = examples[button.dataset.example];
    root
      .querySelectorAll('[data-example]')
      .forEach((el) => el.setAttribute('aria-pressed', String(el === button)));
    root.querySelector('[data-example-help]').textContent =
      button.dataset.example === 'together'
        ? 'A home for chores with your partner or housemates. Invite them after setup.'
        : 'Start with one part of your life. You can add more spaces later.';
    preview();
  }),
);
setup.addEventListener('submit', (event) => {
  event.preventDefault();
  const values = Object.fromEntries(new FormData(setup));
  run(async () => {
    state = await api('/setup', { ...values, revision: state.revision });
    step(2);
  });
});
listForm.addEventListener('submit', (event) => {
  event.preventDefault();
  const values = Object.fromEntries(new FormData(listForm));
  run(async () =>
    leave(await api('/finish', { ...values, revision: state.revision }), true),
  );
});
root.querySelector('[data-welcome-back]').addEventListener('click', () => step(1));
root.querySelector('[data-welcome-skip]').addEventListener('click', () =>
  run(async () => {
    // For a completed account revisiting setup, leaving is a plain navigation.
    if (state.status !== 'pending')
      return location.assign(
        root.dataset.next === '/' ? state.destination : root.dataset.next,
      );
    leave(await api('/skip', { revision: state.revision }));
  }),
);
async function load() {
  reload.hidden = true;
  const wasPending = state?.status === 'pending';
  await run(async () => {
    const data = await api('');
    if (wasPending && data.status !== 'pending')
      leave({ redirect: data.destination }, true);
    else apply(data);
  });
  if (!state) reload.hidden = false;
  else root.querySelector('[data-welcome-skip]').disabled = false;
}
reload.addEventListener('click', load);
load();
