document.documentElement.classList.add('js');
const menu = document.querySelector('.menu-toggle');
const mobileNav = document.querySelector('#mobile-nav');
menu?.addEventListener('click', () => {
  const open = menu.getAttribute('aria-expanded') === 'true';
  menu.setAttribute('aria-expanded', String(!open));
  mobileNav.hidden = open;
});
const demo = document.querySelector('[data-demo]');
if (demo) {
  const count = demo.querySelector('[data-count]');
  const update = () => {
    const tasks = demo.querySelectorAll('.demo-task');
    count.textContent = `${demo.querySelectorAll('.demo-task[aria-pressed="true"]').length} of ${tasks.length} completed`;
  };
  demo.addEventListener('click', (event) => {
    const task = event.target.closest('.demo-task');
    if (task) { task.setAttribute('aria-pressed', String(task.getAttribute('aria-pressed') !== 'true')); update(); }
  });
  const form = demo.querySelector('.demo-form');
  const add = demo.querySelector('.demo-add');
  const input = form.querySelector('input');
  add.addEventListener('click', () => { form.hidden = false; add.hidden = true; input.focus(); });
  form.querySelector('[data-cancel]').addEventListener('click', () => { form.hidden = true; add.hidden = false; input.value = ''; add.focus(); });
  form.addEventListener('submit', (event) => {
    event.preventDefault();
    const title = input.value.trim();
    if (!title) return;
    const task = document.createElement('button');
    task.type = 'button'; task.className = 'demo-task'; task.setAttribute('aria-pressed', 'false');
    const check = document.createElement('span'); check.className = 'task-check';
    const text = document.createElement('span'); text.textContent = title;
    task.append(check, text); demo.querySelector('.demo-tasks').append(task);
    input.value = ''; form.hidden = true; add.hidden = false; update(); add.focus();
  });
}
if ('IntersectionObserver' in window) {
  const observer = new IntersectionObserver((entries) => entries.forEach((entry) => { if (entry.isIntersecting) { entry.target.classList.add('visible'); observer.unobserve(entry.target); } }), {threshold: 0.08});
  document.querySelectorAll('.reveal').forEach((element) => observer.observe(element));
} else document.querySelectorAll('.reveal').forEach((element) => element.classList.add('visible'));
