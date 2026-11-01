export const LIST_COLORS = [
  ['Blue', '#4e89f7'],
  ['Green', '#78b500'],
  ['Yellow', '#fbd919'],
  ['Red', '#ff1818'],
  ['Violet', '#8b67f7'],
  ['Pink', '#ec4899'],
  ['Pearl', '#e2e8f0'],
  ['Teal', '#14b8a6'],
  ['Orange', '#f97316'],
  ['Cyan', '#06b6d4'],
  ['Purple', '#a855f7'],
  ['Rose', '#f43f5e'],
];

// The native color input remains the form control, including for custom colors.
export function mountListColorPicker(
  input,
  { onChange, label = 'Choose list color' } = {},
) {
  const picker = document.createElement('details');
  picker.className = 'list-color-picker';
  const summary = document.createElement('summary');
  summary.setAttribute('aria-label', label);
  summary.title = label;
  const dot = document.createElement('span');
  dot.className = 'list-color-preview';
  summary.append(dot);
  const menu = document.createElement('div');
  menu.className = 'list-color-menu';
  menu.setAttribute('aria-label', 'List colors');
  const grid = document.createElement('div');
  grid.className = 'list-color-grid';
  for (const [name, color] of LIST_COLORS) {
    const choice = document.createElement('button');
    choice.type = 'button';
    choice.className = 'list-color-choice';
    choice.style.backgroundColor = color;
    choice.title = name;
    choice.setAttribute('aria-label', name);
    choice.dataset.color = color;
    choice.addEventListener('click', () => {
      setColor(color);
      picker.open = false;
      onChange?.(color);
    });
    grid.append(choice);
  }
  const custom = document.createElement('label');
  custom.className = 'list-color-custom';
  custom.append('Custom color', input);
  menu.append(grid, custom);
  picker.append(summary, menu);
  document.addEventListener('click', (event) => {
    if (!picker.contains(event.target)) picker.open = false;
  });
  picker.addEventListener('keydown', (event) => {
    if (event.key === 'Escape' && picker.open) {
      event.preventDefault();
      event.stopPropagation();
      picker.open = false;
      summary.focus();
    }
  });
  function setColor(color) {
    input.value = color;
    dot.style.backgroundColor = color;
    for (const choice of grid.children)
      choice.setAttribute(
        'aria-pressed',
        String(choice.dataset.color === color.toLowerCase()),
      );
  }
  input.addEventListener('input', () => setColor(input.value));
  input.addEventListener('change', () => onChange?.(input.value));
  setColor(input.value);
  return {
    element: picker,
    setColor,
    close: () => {
      picker.open = false;
    },
  };
}
