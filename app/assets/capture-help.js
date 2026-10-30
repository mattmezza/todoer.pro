const days = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun'];
const fullDays = [
  'monday',
  'tuesday',
  'wednesday',
  'thursday',
  'friday',
  'saturday',
  'sunday',
];
const dateGroups = (prefix) => [
  {
    label: 'Everyday',
    items: [
      ['today', 'td', 't'],
      ['tomorrow', 'tomo', 'tmr', 'tmrw'],
      ['weekend', 'nextweekend'],
    ].map((aliases) => ({ label: aliases[0], aliases: aliases.map((a) => prefix + a) })),
  },
  {
    label: 'Weekdays',
    items: days.map((day, i) => ({
      label: fullDays[i],
      aliases: [prefix + day, prefix + fullDays[i]],
      note: 'Next occurrence, after today',
    })),
  },
  {
    label: 'Calendar',
    items: [
      {
        label: 'Next week',
        aliases: [prefix + 'nextweek', prefix + 'nw'],
        note: 'Monday of next week',
      },
      {
        label: 'Next month',
        aliases: [prefix + 'nextmonth', prefix + 'nm'],
        note: 'First day of next month',
      },
      {
        label: 'Month and day',
        aliases: [prefix + 'oct31', prefix + '31oct', prefix + 'october31'],
        note: 'Next occurrence of this calendar date',
      },
      { label: 'Exact date', aliases: [prefix + '2026-12-31'], note: 'Use YYYY-MM-DD' },
    ],
  },
  {
    label: 'Relative dates',
    items: [
      {
        label: 'Days from today',
        aliases: [prefix + 'in3days', prefix + 'in3d'],
        note: 'Replace 3 with the number of days',
      },
      {
        label: 'Weeks from today',
        aliases: [prefix + 'in1week', prefix + 'in2w'],
        note: 'Replace the number for any week offset',
      },
      {
        label: 'Months from today',
        aliases: [prefix + 'in1month', prefix + 'in2m'],
        note: 'Clamps to the last day in shorter months',
      },
      {
        label: 'Years from today',
        aliases: [prefix + 'in1year', prefix + 'in2y'],
        note: 'Up to ten years ahead',
      },
    ],
  },
];
export function mountCaptureHelp({ scoped, allowSection, insert, datesEnabled }) {
  const help = document.createElement('div');
  help.className = 'capture-help';
  const button = document.createElement('button');
  button.type = 'button';
  button.textContent = 'Shortcuts';
  button.setAttribute('aria-haspopup', 'dialog');
  const menu = document.createElement('div');
  menu.className = 'capture-shortcut-menu';
  menu.popover = 'auto';
  menu.id = `capture-help-${crypto.randomUUID()}`;
  menu.setAttribute('role', 'dialog');
  menu.setAttribute('aria-label', 'Capture shortcuts');
  button.setAttribute('aria-controls', menu.id);
  help.append(button, menu);
  let history = [];
  const categories = () => [
    ...(datesEnabled()
      ? [
          { label: 'Planned date', items: dateGroups('.') },
          { label: 'Due date', items: dateGroups('-') },
        ]
      : []),
    {
      label: 'Priority',
      items: [
        { label: 'Highest', aliases: ['p0', '!0', '!!!'] },
        { label: 'High', aliases: ['p1', '!1', '!!'] },
        { label: 'Medium', aliases: ['p2', '!2', '!'] },
        { label: 'Low', aliases: ['p3', '!3'] },
      ],
    },
    {
      label: 'Tags',
      items: [
        { label: 'Tag', aliases: ['+family'] },
        { label: 'Tag containing spaces', aliases: ['+"weekly shopping"'] },
      ],
    },
    ...(!scoped
      ? [
          {
            label: 'Space and list',
            items: [
              { label: 'Space', aliases: ['@personal'] },
              { label: 'List', aliases: ['#groceries'] },
              { label: 'Names containing spaces', aliases: ['@"work space"'] },
            ],
          },
        ]
      : []),
    ...(allowSection
      ? [
          {
            label: 'Section',
            items: [{ label: 'Section in this list', aliases: ['$veggies'] }],
          },
        ]
      : []),
  ];
  function position() {
    const anchor = button.getBoundingClientRect();
    menu.style.left = `${Math.max(16, Math.min(anchor.left, innerWidth - menu.offsetWidth - 16))}px`;
    menu.style.top = `${Math.max(16, Math.min(anchor.top - menu.offsetHeight - 8, innerHeight - menu.offsetHeight - 16))}px`;
  }
  function render() {
    menu.replaceChildren();
    const current = history.at(-1);
    const heading = document.createElement('div');
    heading.className = 'capture-shortcut-heading';
    if (current) {
      const back = document.createElement('button');
      back.type = 'button';
      back.textContent = '← Back';
      back.setAttribute('aria-label', 'Back to previous shortcuts');
      back.onclick = () => {
        history.pop();
        render();
        menu.querySelector('button')?.focus();
      };
      heading.append(back);
    }
    const title = document.createElement('strong');
    title.textContent = current?.label || 'Shortcuts';
    heading.append(title);
    menu.append(heading);
    for (const item of current?.items || categories()) {
      if (item.items) {
        const next = document.createElement('button');
        next.type = 'button';
        next.className = 'capture-shortcut-category';
        next.textContent = `${item.label} →`;
        next.onclick = () => {
          history.push(item);
          render();
          menu.querySelector('button')?.focus();
        };
        menu.append(next);
      } else {
        const row = document.createElement('div');
        row.className = 'capture-shortcut-row';
        const label = document.createElement('span');
        label.textContent = item.label;
        row.append(label);
        const aliases = document.createElement('div');
        for (const alias of item.aliases) {
          const pick = document.createElement('button');
          pick.type = 'button';
          pick.textContent = alias;
          pick.onclick = () => {
            menu.hidePopover();
            insert(alias);
          };
          aliases.append(pick);
        }
        row.append(aliases);
        if (item.note) {
          const note = document.createElement('small');
          note.textContent = item.note;
          row.append(note);
        }
        menu.append(row);
      }
    }
    if (menu.matches(':popover-open')) position();
  }
  button.onclick = () => {
    if (menu.matches(':popover-open')) menu.hidePopover();
    else {
      history = [];
      render();
      menu.showPopover();
      position();
    }
  };
  menu.addEventListener('toggle', () =>
    button.setAttribute('aria-expanded', String(menu.matches(':popover-open'))),
  );
  return help;
}
