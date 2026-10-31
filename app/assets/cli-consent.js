const all = document.querySelector('[data-cli-all]');
const resources = document.querySelector('.cli-resources');
if (all && resources) {
  const update = () => {
    resources.querySelectorAll('input').forEach((input) => {
      input.disabled = all.checked;
    });
    resources.open = !all.checked;
  };
  all.addEventListener('change', update);
  update();
}
