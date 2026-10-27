function mountCards() {
  const toy = document.createElement('button');
  toy.className = 'task-play';
  toy.type = 'button';
  toy.setAttribute('aria-label', 'Playfully check off a little task');
  toy.innerHTML =
    '<span class="play-card play-card-back" aria-hidden="true"></span><span class="play-card" aria-hidden="true"><span class="play-check">✓</span><span class="play-line"></span><span class="play-line short"></span></span>';
  let launching = false;
  toy.addEventListener('click', async () => {
    if (launching) return;
    launching = true;
    toy.classList.add('is-checked');
    const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
    await new Promise((resolve) => setTimeout(resolve, reduced ? 220 : 110));
    if (!toy.isConnected) {
      launching = false;
      return;
    }
    if (!reduced) {
      const card = toy.querySelector('.play-card:not(.play-card-back)');
      const rect = card.getBoundingClientRect();
      const flyer = card.cloneNode(true);
      flyer.classList.add('task-flyer');
      flyer.setAttribute('aria-hidden', 'true');
      Object.assign(flyer.style, {
        left: `${rect.left}px`,
        top: `${rect.top}px`,
        width: `${rect.width}px`,
        height: `${rect.height}px`,
      });
      document.body.append(flyer);
      // Fire through an upward semicircle, with varied speed and spin.
      const angle = ((-165 + Math.random() * 150) * Math.PI) / 180;
      const speed = 650 + Math.random() * 850;
      const duration = 650 + Math.random() * 430;
      const distance = (speed * duration) / 1000;
      const x = Math.cos(angle) * distance;
      const y = Math.sin(angle) * distance;
      const spin = (Math.random() > 0.5 ? 1 : -1) * (160 + Math.random() * 440);
      flyer
        .animate(
          [
            { transform: 'translate(0, 0) rotate(-8deg) scale(1)', opacity: 1 },
            {
              transform: `translate(${x * 0.35}px, ${y * 0.48}px) rotate(${spin * 0.35}deg) scale(1.08)`,
              opacity: 1,
              offset: 0.4,
            },
            {
              transform: `translate(${x}px, ${y}px) rotate(${spin}deg) scale(.55)`,
              opacity: 0,
            },
          ],
          { duration, easing: 'cubic-bezier(.12,.55,.32,1)', fill: 'forwards' },
        )
        .finished.finally(() => flyer.remove());
      card.animate(
        [
          { opacity: 0, transform: 'translateY(12px) scale(.85)' },
          { opacity: 1, transform: 'translateY(0) scale(1)' },
        ],
        {
          duration: 240,
          delay: 60,
          fill: 'backwards',
          easing: 'cubic-bezier(.2,.8,.2,1)',
        },
      );
    }
    toy.classList.remove('is-checked');
    launching = false;
  });
  document.body.append(toy);
  return toy;
}

// A shuffled bag keeps the jokes varied without repeating the visible rows.
const tinyTasks = [
  'Find the tab playing music',
  'Rename final_final_v7',
  'Unsubscribe from my own thoughts',
  'Ask the printer how it feels',
  'Schedule a meeting with my plant',
  'Turn coffee into plausible plans',
  'Locate the other sock',
  'Give inbox zero a participation trophy',
  'Put “relax” on tomorrow’s list',
  'Tell the laundry we need space',
  'Close a tab. Open three more.',
  'Let the loading spinner finish its dance',
  'Water the plastic plant. Just in case.',
  'Negotiate with the snooze button',
  'Move this task to a nicer list',
  'Check if the fridge has new content',
  'Promote a snack to a full meal',
  'Reboot my sense of purpose',
  'Send “sounds good” with confidence',
  'Convince the cat I pay rent here',
  'Find the end of the fitted sheet',
  'Take a break from planning my breaks',
  'Give that chair its clothes back',
  'Tell the spreadsheet it’s doing great',
  'Remember why I entered this room',
  'Make eye contact with the unread emails',
  'Update the software. Emotionally.',
  'Accept cookies. Find actual cookies.',
  'Thank past me for absolutely nothing',
  'Circle back to the snack drawer',
  'Pretend the walk was for exercise',
  'Put the “pro” in procrastination',
  'Turn “quick question” into a novella',
  'Decline a meeting with my inner critic',
  'Let the dishes soak for one more era',
  'Give my browser tabs assigned seating',
];
function flyFunnyTask(row) {
  const rect = row.getBoundingClientRect();
  const flyer = row.cloneNode(true);
  flyer.className = 'fun-task-row fun-task-active is-done fun-task-flyer';
  flyer.removeAttribute('aria-label');
  flyer.setAttribute('aria-hidden', 'true');
  flyer.tabIndex = -1;
  Object.assign(flyer.style, {
    left: rect.left + 'px',
    top: rect.top + 'px',
    width: rect.width + 'px',
    height: rect.height + 'px',
  });
  document.body.append(flyer);
  const flyers = document.querySelectorAll('.fun-task-flyer');
  if (flyers.length > 12) flyers[0].remove();
  const flat = -innerWidth * 0.48;
  const extra = -innerWidth * (0.35 + Math.random() * 0.25);
  const angle = ((18 + Math.random() * 40) * Math.PI) / 180;
  const y = Math.abs(extra) * Math.tan(angle) * (Math.random() < 0.5 ? -1 : 1);
  const spin = (Math.random() < 0.5 ? -1 : 1) * (12 + Math.random() * 18);
  flyer
    .animate(
      [
        { transform: 'translate(0,0)', opacity: 0.7 },
        { transform: 'translate(' + flat + 'px,0)', opacity: 0.6, offset: 0.48 },
        {
          transform:
            'translate(' +
            (flat + extra * 0.4) +
            'px,' +
            y * 0.25 +
            'px) rotate(' +
            spin * 0.35 +
            'deg)',
          opacity: 0.35,
          offset: 0.73,
        },
        {
          transform:
            'translate(' + (flat + extra) + 'px,' + y + 'px) rotate(' + spin + 'deg)',
          opacity: 0,
        },
      ],
      { duration: 580 + Math.random() * 200, easing: 'linear' },
    )
    .finished.catch(() => {})
    .finally(() => flyer.remove());
}
function mountFunnyTasks() {
  const tray = document.createElement('div');
  tray.className = 'fun-task-stack';
  tray.setAttribute('aria-label', 'A little comic relief');
  let bag = [],
    visible = [],
    fromTop = true,
    busy = false,
    queued = 0;
  function next() {
    if (!bag.length) {
      bag = tinyTasks.filter((text) => !visible.includes(text));
      for (let i = bag.length - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        [bag[i], bag[j]] = [bag[j], bag[i]];
      }
    }
    return bag.pop();
  }
  visible = [next(), next(), next()];
  const rows = ['top', 'active', 'bottom'].map((position, index) => {
    const row = document.createElement(index === 1 ? 'button' : 'div');
    row.className = 'fun-task-row fun-task-' + position;
    if (index === 1) row.type = 'button';
    else row.setAttribute('aria-hidden', 'true');
    const check = document.createElement('span');
    check.className = 'fun-task-check';
    check.setAttribute('aria-hidden', 'true');
    check.textContent = '✓';
    const text = document.createElement('span');
    text.className = 'fun-task-text';
    row.append(check, text);
    tray.append(row);
    return row;
  });
  function render() {
    rows.forEach(
      (row, index) => (row.querySelector('.fun-task-text').textContent = visible[index]),
    );
    rows[1].setAttribute('aria-label', 'Just for fun: ' + visible[1]);
    rows[1].classList.remove('is-done');
  }
  render();
  rows[1].onclick = async () => {
    if (busy) {
      queued++;
      return;
    }
    busy = true;
    const center = rows[1],
      entering = rows[fromTop ? 0 : 2];
    const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
    center.classList.add('is-done');
    tray.dataset.direction = fromTop ? 'top' : 'bottom';
    try {
      await new Promise((resolve) => setTimeout(resolve, reduced ? 0 : 25));
      if (!tray.isConnected) return;
      if (!reduced) {
        flyFunnyTask(center);
        const dy =
          center.getBoundingClientRect().top - entering.getBoundingClientRect().top;
        await Promise.all([
          center.animate(
            [
              { opacity: 1, transform: 'scale(1)' },
              {
                opacity: 0,
                transform: 'translateX(-24px)',
                filter: 'blur(3px)',
              },
            ],
            { duration: 110, easing: 'cubic-bezier(.2,.7,.2,1)' },
          ).finished,
          entering.animate(
            [
              { opacity: 0.2, transform: 'translateY(0) scale(1)' },
              { opacity: 1, transform: 'translateY(' + dy + 'px) scale(1)' },
            ],
            { duration: 110, easing: 'cubic-bezier(.2,.8,.2,1)' },
          ).finished,
        ]);
      }
      visible = fromTop
        ? [next(), visible[0], visible[2]]
        : [visible[0], visible[2], next()];
      fromTop = !fromTop;
      render();
      if (!reduced)
        rows[fromTop ? 2 : 0].animate([{ opacity: 0 }, { opacity: 0.2 }], {
          duration: 90,
        });
    } finally {
      busy = false;
      if (queued > 0 && tray.isConnected) {
        queued--;
        rows[1].click();
      }
    }
  };
  const tick = setInterval(async () => {
    if (!tray.isConnected) {
      clearInterval(tick);
      return;
    }
    if (
      busy ||
      document.hidden ||
      tray.matches(':hover,:focus-within') ||
      matchMedia('(prefers-reduced-motion: reduce)').matches
    )
      return;
    busy = true;
    try {
      const dy =
        rows[1].getBoundingClientRect().top - rows[2].getBoundingClientRect().top;
      await Promise.all(
        rows.map(
          (row, index) =>
            row.animate(
              [
                { transform: 'translateY(0)', opacity: index === 1 ? 1 : 0.2 },
                {
                  transform: 'translateY(' + (dy - 2) + 'px)',
                  opacity: index === 2 ? 1 : index === 1 ? 0.2 : 0,
                  offset: 0.82,
                },
                {
                  transform: 'translateY(' + dy + 'px)',
                  opacity: index === 2 ? 1 : index === 1 ? 0.2 : 0,
                },
              ],
              { duration: 280, easing: 'cubic-bezier(.16,1,.3,1)' },
            ).finished,
        ),
      );
      visible = [visible[1], visible[2], next()];
      render();
    } finally {
      busy = false;
      if (queued > 0 && tray.isConnected) {
        queued--;
        rows[1].click();
      }
    }
  }, 2600);
  document.body.append(tray);
  return tray;
}
if (
  document.body.classList.contains('planning-list-page') &&
  document.querySelector('meta[name=account-id]')
) {
  let toy;
  function apply(value) {
    toy?.remove();
    document
      .querySelectorAll('.task-flyer,.fun-task-flyer')
      .forEach((flyer) => flyer.remove());
    toy =
      value.fun_animations === false
        ? null
        : value.fun_animation === 'tasks'
          ? mountFunnyTasks()
          : mountCards();
  }
  document.addEventListener('todoer-play-preferences', (event) => apply(event.detail));
  fetch('/api/v1/notification-preferences', { cache: 'no-store' })
    .then((response) => (response.ok ? response.json() : null))
    .then((value) => {
      if (value) apply(value);
    })
    .catch(() => {});
}
