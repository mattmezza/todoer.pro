import Alpine from './alpine-csp.min.js';

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

const configurations = new WeakMap();

export function listColorPicker() {
  let onChange;
  let picker;
  return {
    color: '#4e89f7',
    label: 'Choose list color',
    colors: LIST_COLORS.map(([name, color]) => ({
      name,
      color,
      style: { backgroundColor: color },
    })),
    init() {
      picker = this.$el;
      const options = configurations.get(picker);
      onChange = options?.onChange;
      this.label = options?.label || this.label;
      this.color = picker.querySelector('input').value.toLowerCase();
    },
    get previewStyle() {
      return { backgroundColor: this.color };
    },
    setColor(color) {
      this.$refs.input.value = color;
      this.color = this.$refs.input.value.toLowerCase();
    },
    close() {
      picker.open = false;
    },
    escape(event) {
      if (!picker.open) return;
      event.preventDefault();
      event.stopPropagation();
      this.close();
      this.$refs.summary.focus();
    },
    choose(event) {
      this.setColor(event.currentTarget.dataset.color);
      this.close();
      this.$dispatch('list-color-change');
      onChange?.(this.color);
    },
    customInput() {
      this.color = picker.querySelector('input').value.toLowerCase();
      this.$dispatch('list-color-change');
    },
    customChange() {
      onChange?.(this.color);
    },
  };
}

// Bridge the existing list editors to the same declarative picker used in forms.
export function mountListColorPicker(input, options = {}) {
  const template = document.querySelector('#list-color-picker-template');
  const element = template.content.firstElementChild.cloneNode(true);
  const placeholder = element.querySelector('input');
  for (const { name, value } of placeholder.attributes)
    if (name.startsWith('x-')) input.setAttribute(name, value);
  placeholder.replaceWith(input);
  configurations.set(element, options);
  Alpine.initTree(element);
  const component = Alpine.$data(element);
  return {
    element,
    setColor: (color) => component.setColor(color),
    close: () => component.close(),
  };
}
