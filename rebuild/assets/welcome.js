import { request } from './request.js';
const root = document.querySelector('[data-welcome]');
const setup = root.querySelector('[data-welcome-setup]');
const taskForm = root.querySelector('[data-welcome-task]');
const error = root.querySelector('[data-welcome-error]');
const reload = root.querySelector('[data-welcome-reload]');
const heading = root.querySelector('h1');
const csrf = document.querySelector('meta[name="csrf-token"]').content;
let state,
  busy = false;
const palette = ['blue', 'violet', 'green', 'amber', 'pink'];
function preview() {
  root.querySelector('[data-preview-space]').textContent =
    setup.elements.space.value.trim() || 'Your space';
  root.querySelector('[data-preview-list]').textContent =
    setup.elements.list.value.trim() || 'Your list';
  root.querySelector('[data-preview-task]').textContent =
    taskForm.elements.task.value.trim() || 'Your first task';
  const color = setup.elements.color.value || 'blue';
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
  root.querySelector('[data-preview-planned]').hidden = !taskForm.elements.today.checked;
  root.querySelector('[data-welcome-finish]').textContent =
    taskForm.elements.task.value.trim() ? 'Add task & get started →' : 'Get started →';
}
function step(number, focus = true) {
  root.dataset.step = number;
  setup.hidden = number !== 1;
  taskForm.hidden = number !== 2;
  root.querySelector('[data-welcome-count]').textContent = `Step ${number} of 2`;
  root
    .querySelectorAll('.welcome-progress i')
    .forEach((dot, index) => dot.classList.toggle('active', index < number));
  heading.textContent =
    number === 1 ? 'A little space for what matters.' : 'One small thing. A good start.';
  root.querySelector('[data-welcome-intro]').textContent =
    number === 1
      ? 'Life, work, and the things you share. Make this corner yours.'
      : 'Give that thought a place. You don’t have to plan everything today.';
  root.querySelector('[data-preview-teaching]').textContent =
    number === 1
      ? 'Spaces group your lists. Lists hold your tasks. Enough structure to make things feel lighter.'
      : 'Give tasks a home, then make time for them. Reminders and AI assistance are there to explore when you’re ready.';
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
  setup.elements.space.value = data.space;
  setup.elements.list.value = data.list;
  if (
    !palette.includes(data.color) &&
    (/^#[a-f0-9]{6}$/i.test(data.color) || ['red', 'gray'].includes(data.color))
  ) {
    setup.querySelectorAll('[data-custom-color]').forEach((el) => el.remove());
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
    setup.querySelector('fieldset').append(label);
  }
  setup.elements.color.value = data.color || 'blue';
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
    taskForm.hidden = true;
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
taskForm.addEventListener('input', preview);
root.querySelectorAll('[data-example]').forEach((button) =>
  button.addEventListener('click', () => {
    const examples = {
      personal: ['Personal', 'Inbox'],
      work: ['Work', 'Priorities'],
      together: ['Our space', 'Shared plans'],
    };
    [setup.elements.space.value, setup.elements.list.value] =
      examples[button.dataset.example];
    root
      .querySelectorAll('[data-example]')
      .forEach((el) => el.setAttribute('aria-pressed', String(el === button)));
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
async function finish(later = false) {
  const task = later ? '' : taskForm.elements.task.value;
  const today = !later && taskForm.elements.today.checked;
  await run(async () =>
    leave(await api('/finish', { revision: state.revision, task, today }), true),
  );
}
taskForm.addEventListener('submit', (event) => {
  event.preventDefault();
  finish();
});
root.querySelector('[data-welcome-later]').addEventListener('click', () => finish(true));
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
