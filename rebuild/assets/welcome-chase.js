// A tiny, dependency-free 3D-projected character. The gait uses two-link leg IK:
// stance feet move backward at ground speed, then lift through a smooth swing.
// Decorative only: it never touches real tasks or intercepts input.
export function welcomeChase() {
  const desktop = matchMedia(
    '(min-width: 1024px) and (pointer: fine), (min-width: 1024px) and (pointer: none)',
  );
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  const canvas = document.createElement('canvas');
  const ctx = canvas.getContext('2d');
  if (!ctx) return { finish: () => Promise.resolve() };
  canvas.className = 'welcome-chase';
  canvas.setAttribute('aria-hidden', 'true');
  canvas.dataset.phase = 'chase';
  canvas.dataset.complete = 'false';
  let width = innerWidth,
    frame,
    last,
    time = 0,
    modeTime = 0;
  let human = 54,
    task = human + 92,
    direction = 1,
    facing = 1;
  let velocity = 0,
    gait = 0,
    rest = false,
    jump = 0,
    jumpAge = 1,
    jumpLength = 0.8;
  let finishTime = null,
    caughtAt = null,
    resolveFinish,
    finishPromise,
    finishTimeout;
  let particles = [];
  // Use the empty left gutter, so the chase never runs across the setup choices.
  let trackWidth = Math.max(190, (width - 600) / 2 - 20);
  const enabled = () => desktop.matches && !reduced.matches;
  const clamp = (value, min, max) => Math.max(min, Math.min(max, value));
  const mix = (a, b, t) => a + (b - a) * t;
  const smooth = (t) => t * t * (3 - 2 * t);
  function resize() {
    width = innerWidth;
    trackWidth = Math.max(190, (width - 600) / 2 - 20);
    const ratio = Math.min(devicePixelRatio || 1, 2);
    canvas.width = Math.round(width * ratio);
    canvas.height = Math.round(128 * ratio);
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    human = clamp(human, 48, trackWidth - 48);
    task = clamp(task, 32, trackWidth - 32);
  }
  function ellipse(x, y, rx, ry, fill) {
    ctx.beginPath();
    ctx.ellipse(x, y, rx, ry, 0, 0, Math.PI * 2);
    ctx.fillStyle = fill;
    ctx.fill();
  }
  function capsule(a, b, radius, colors) {
    const dx = b[0] - a[0],
      dy = b[1] - a[1],
      length = Math.hypot(dx, dy) || 1;
    const nx = (-dy / length) * radius,
      ny = (dx / length) * radius;
    const light = ctx.createLinearGradient(a[0] - nx, a[1] - ny, a[0] + nx, a[1] + ny);
    light.addColorStop(0, colors[0]);
    light.addColorStop(0.38, colors[1]);
    light.addColorStop(1, colors[2]);
    ctx.lineWidth = radius * 2;
    ctx.lineCap = 'round';
    ctx.strokeStyle = light;
    ctx.beginPath();
    ctx.moveTo(...a);
    ctx.lineTo(...b);
    ctx.stroke();
  }
  function sphere(x, y, r, colors) {
    const light = ctx.createRadialGradient(x - r * 0.35, y - r * 0.4, r * 0.1, x, y, r);
    light.addColorStop(0, colors[0]);
    light.addColorStop(0.65, colors[1]);
    light.addColorStop(1, colors[2]);
    ellipse(x, y, r, r, light);
  }
  function knee(hip, foot, bend) {
    const dx = foot[0] - hip[0],
      dy = foot[1] - hip[1];
    const d = Math.min(Math.hypot(dx, dy), 30.9),
      h = Math.sqrt(Math.max(0, 15.5 ** 2 - (d * d) / 4));
    const length = Math.hypot(dx, dy) || 1;
    return [
      (hip[0] + foot[0]) / 2 - (dy / length) * h * bend,
      (hip[1] + foot[1]) / 2 + (dx / length) * h * bend,
    ];
  }
  function drawHuman(celebration) {
    const running = clamp(Math.abs(velocity) / 100, 0, 1);
    const breath = Math.sin(time * 3) * (1 - running) * 0.8;
    const bob = running * (1.3 + Math.cos(gait * 2) * 1.3);
    const joy = celebration
      ? Math.max(0, Math.sin(celebration * 9)) * 10 * Math.max(0, 1 - celebration / 2)
      : 0;
    const lean = running * 5 + (rest && !celebration ? 3 : 0);
    const ground = 125;
    ellipse(human, ground - 1, 18 - joy * 0.2, 2.5, '#00000030');
    // A slight depth offset exposes both sides rather than a flat stick figure.
    const p = (x, y, z = 0) => [
      human + facing * (x + z * 0.28),
      ground - y - joy + z * 0.12,
    ];
    const skin = ['#f8d8b5', '#eeb88d', '#b97557'];
    const farSkin = ['#dca27d', '#cc926e', '#955e4c'];
    const blue = ['#6588ca', '#4269ac', '#253b71'];
    const farBlue = ['#3e5e96', '#2b477d', '#1e3057'];
    function leg(offset, depth) {
      const phase = (((gait / (Math.PI * 2) + offset) % 1) + 1) % 1;
      const stance = 0.56;
      const stride = 25 * running;
      const x =
        phase < stance
          ? stride * (0.5 - phase / stance)
          : -stride * 0.5 + stride * smooth((phase - stance) / (1 - stance));
      const lift =
        phase < stance
          ? 0
          : Math.sin(((phase - stance) / (1 - stance)) * Math.PI) * 15 * running;
      const hip = [0, 31 + bob + breath],
        foot = [x + (1 - running) * (offset ? -4 : 4), 3 + lift];
      const k = knee(hip, foot, 1);
      const colors = depth < 0 ? farBlue : blue;
      capsule(p(...hip, depth), p(...k, depth), 3.5, colors);
      capsule(p(...k, depth), p(...foot, depth), 2.8, colors);
      const shoe = p(foot[0] + 2, foot[1] - 0.5, depth);
      capsule([shoe[0] - facing * 2, shoe[1]], [shoe[0] + facing * 5, shoe[1]], 2.5, [
        '#f5f1e8',
        '#dddde2',
        '#919baa',
      ]);
      capsule(
        [shoe[0] - facing * 2, shoe[1] + 1.6],
        [shoe[0] + facing * 5, shoe[1] + 1.6],
        0.65,
        ['#fff', '#fff', '#b8c2d3'],
      );
    }
    function arm(side, depth) {
      const swing = Math.sin(gait + (side ? Math.PI : 0)) * running;
      const shoulder = [lean, 51 + bob + breath];
      let elbow = [lean + swing * 10 - 2, 41 + bob + breath];
      let hand = [elbow[0] + 6, elbow[1] + 5];
      if (rest && !celebration && running < 0.1) {
        elbow = [7, 40 + breath];
        hand = [5, 32 + breath];
      }
      if (finishTime !== null && caughtAt === null && Math.abs(task - human) < 55) {
        elbow = [14, 44];
        hand = [25, 43];
      }
      if (celebration) {
        elbow = [side ? 12 : -10, 60];
        hand = [side ? 17 : -13, 71 + Math.sin(celebration * 12 + side) * 2];
      }
      capsule(p(...shoulder, depth), p(...elbow, depth), 2.8, depth < 0 ? farSkin : skin);
      capsule(p(...elbow, depth), p(...hand, depth), 2.5, depth < 0 ? farSkin : skin);
      const h = p(...hand, depth);
      sphere(...h, 2.7, depth < 0 ? farSkin : skin);
    }
    arm(0, -5);
    leg(0.5, -4);
    capsule(p(0, 31), p(lean, 49), 7, ['#ffcc78', '#f7a84a', '#bf6f2d']);
    capsule(p(0, 30), p(1, 34), 6, blue);
    leg(0, 4);
    capsule(p(lean, 51), p(lean + 1, 57), 3, skin);
    const head = p(lean + 1, 63 + bob + breath);
    sphere(...head, 8, skin);
    // Sculpted hair cap, ear and a small profile nose.
    ctx.save();
    ctx.beginPath();
    ctx.arc(head[0], head[1], 8.3, 0, Math.PI * 2);
    ctx.clip();
    ellipse(head[0] - facing * 3, head[1] - 5.5, 9, 6, '#473b3b');
    ellipse(head[0] - facing * 6, head[1] - 1.5, 3, 5, '#392f35');
    ctx.restore();
    sphere(head[0] - facing * 1.5, head[1] + 1, 2.1, skin);
    sphere(head[0] + facing * 7.2, head[1] + 0.8, 2, skin);
    ellipse(head[0] + facing * 4.3, head[1] - 1, 0.8, 1, '#302b39');
    ctx.strokeStyle = '#9c5e52';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.arc(head[0] + facing * 4, head[1] + 2.5, 2, 0.15, 1.9);
    ctx.stroke();
    arm(1, 5);
    if (rest && !celebration && running < 0.1) {
      // A tiny exhale, instead of text or UI competing with the form.
      const puff = (time * 0.7) % 1;
      ellipse(
        head[0] + facing * (14 + puff * 12),
        head[1] + 4 - puff * 7,
        2 + puff * 2,
        1.4,
        `rgba(148,163,184,${0.35 * (1 - puff)})`,
      );
    }
  }
  function drawTask(celebration) {
    const checked = caughtAt !== null;
    const tease = rest && !checked;
    const bounce = tease ? Math.abs(Math.sin(time * 5)) * 7 : 0;
    const y =
      125 -
      23 -
      jump -
      bounce -
      (celebration ? Math.sin(Math.min(1, celebration) * Math.PI) * 9 : 0);
    ellipse(task, 124, 20 - jump * 0.15, 2.5, `rgba(0,0,0,${0.18 - jump * 0.002})`);
    ctx.save();
    ctx.translate(task, y);
    ctx.rotate(
      tease
        ? Math.sin(time * 5) * 0.13
        : jump > 0
          ? Math.sin((jumpAge / jumpLength) * Math.PI) * direction * -0.2
          : Math.sin(time * 3) * 0.025,
    );
    // Extruded edge and a shaded paper face give the little task some volume.
    ctx.fillStyle = checked ? '#21876b' : '#6281b4';
    ctx.beginPath();
    ctx.roundRect(-22, -17, 46, 33, 7);
    ctx.fill();
    const paper = ctx.createLinearGradient(-20, -18, 20, 16);
    paper.addColorStop(0, checked ? '#b0f7d3' : '#f4f7ff');
    paper.addColorStop(1, checked ? '#6bd7aa' : '#cbd9ef');
    ctx.fillStyle = paper;
    ctx.beginPath();
    ctx.roundRect(-24, -20, 46, 33, 6);
    ctx.fill();
    ctx.strokeStyle = checked ? '#167b5c' : '#7e96ba';
    ctx.lineWidth = 1.6;
    ctx.beginPath();
    ctx.arc(-12, -3, 5.7, 0, Math.PI * 2);
    ctx.stroke();
    if (checked) {
      ctx.strokeStyle = '#14714e';
      ctx.lineWidth = 2.4;
      ctx.lineJoin = 'round';
      ctx.beginPath();
      ctx.moveTo(-15, -3);
      ctx.lineTo(-12, 0);
      ctx.lineTo(-7, -6);
      ctx.stroke();
    }
    // Eyes and a cheeky smile; hands wave only while the runner rests.
    ellipse(4, -6, 1.2, 1.8, '#405676');
    ellipse(12, -6, 1.2, 1.8, '#405676');
    ctx.strokeStyle = '#405676';
    ctx.lineWidth = 1.3;
    ctx.beginPath();
    ctx.arc(8, -3, 4, 0.1, Math.PI - 0.1);
    ctx.stroke();
    if (tease || checked) {
      for (const side of [-1, 1]) {
        ctx.strokeStyle = checked ? '#47b98d' : '#8da9d1';
        ctx.lineWidth = 2;
        ctx.lineCap = 'round';
        ctx.beginPath();
        ctx.moveTo(side * 22, -3);
        ctx.quadraticCurveTo(
          side * 28,
          -9,
          side * 30,
          -14 + Math.sin(time * 9 + side) * 4,
        );
        ctx.stroke();
      }
    }
    ctx.restore();
  }
  function confetti() {
    particles = Array.from({ length: 30 }, (_, i) => ({
      x: task,
      y: 87,
      vx: Math.cos(i * 2.4) * (35 + (i % 5) * 15),
      vy: -65 - (i % 7) * 15,
      age: 0,
      color: ['#60a5fa', '#a78bfa', '#34d399', '#fbbf24', '#f472b6'][i % 5],
    }));
  }
  function done() {
    clearTimeout(finishTimeout);
    if (resolveFinish) {
      resolveFinish();
      resolveFinish = null;
    }
    canvas.dataset.phase = 'done';
  }
  function tick(now) {
    frame = null;
    if (!enabled() || document.hidden) return;
    const dt = Math.min((now - (last || now)) / 1000, 0.04);
    last = now;
    time += dt;
    modeTime += dt;
    if (finishTime === null) {
      if (modeTime > (rest ? 4 : 10)) {
        rest = !rest;
        modeTime = 0;
        canvas.dataset.phase = rest ? 'rest' : 'chase';
      }
      if (!rest) {
        const gap = (task - human) * direction;
        // Slow down just enough for a near miss, then leap and escape.
        let taskSpeed = modeTime % 4.6 < 2.8 ? 88 : 40;
        if (gap < 53 && jumpAge >= jumpLength) {
          jumpAge = 0;
          jumpLength = 0.85;
        }
        if (jumpAge < jumpLength) taskSpeed = 145;
        task += direction * taskSpeed * dt;
        if ((task > trackWidth - 40 && direction > 0) || (task < 40 && direction < 0)) {
          // Jump over the runner to turn around at the edge of the window.
          direction *= -1;
          jumpAge = 0;
          jumpLength = 1.1;
        }
        const desired = direction * (gap > 170 ? 128 : 105);
        velocity = mix(velocity, desired, Math.min(1, dt * 4));
      } else {
        velocity *= Math.exp(-dt * 5);
        const target = clamp(human + facing * 80, 40, trackWidth - 40);
        task = mix(task, target, Math.min(1, dt * 2));
      }
      jumpAge += dt;
      jump = jumpAge < jumpLength ? Math.sin((jumpAge / jumpLength) * Math.PI) * 53 : 0;
    } else {
      finishTime += dt;
      jump = mix(jump, 0, Math.min(1, dt * 10));
      rest = false;
      if (caughtAt === null) {
        const distance = task - human;
        direction = Math.sign(distance) || direction;
        velocity = mix(
          velocity,
          direction * Math.min(175, Math.max(45, (Math.abs(distance) - 28) * 5)),
          Math.min(1, dt * 7),
        );
        if (Math.abs(distance) < 34 || finishTime > 1.7) {
          human = task - direction * 32;
          velocity = 0;
          caughtAt = time;
          canvas.dataset.phase = 'celebrate';
          canvas.dataset.complete = 'true';
          confetti();
        }
      } else {
        velocity = 0;
        if (time - caughtAt > 1.7) done();
      }
    }
    human = clamp(human + velocity * dt, 40, trackWidth - 40);
    task = clamp(task, 30, trackWidth - 30);
    if (Math.abs(velocity) > 5)
      facing = mix(facing, Math.sign(velocity), Math.min(1, dt * 10));
    // A 25px stance across 56% of a cycle tracks the actual world speed.
    gait +=
      ((Math.abs(velocity) * dt * 0.56) /
        (25 * Math.max(0.05, Math.min(1, Math.abs(velocity) / 100)))) *
      Math.PI *
      2;
    ctx.clearRect(0, 0, width, 128);
    const celebration = caughtAt === null ? 0 : time - caughtAt + 0.001;
    drawHuman(celebration);
    drawTask(celebration);
    for (const particle of particles) {
      particle.age += dt;
      particle.vy += 130 * dt;
      particle.x += particle.vx * dt;
      particle.y += particle.vy * dt;
      ctx.globalAlpha = Math.max(0, 1 - particle.age / 1.7);
      ctx.fillStyle = particle.color;
      ctx.save();
      ctx.translate(particle.x, particle.y);
      ctx.rotate(particle.age * 5);
      ctx.fillRect(-2, -1, 4, 2);
      ctx.restore();
    }
    ctx.globalAlpha = 1;
    if (canvas.dataset.phase !== 'done') frame = requestAnimationFrame(tick);
  }
  function sync() {
    if (frame) cancelAnimationFrame(frame);
    frame = null;
    last = null;
    if (enabled()) {
      if (!canvas.isConnected) document.body.append(canvas);
      resize();
      if (!document.hidden && canvas.dataset.phase !== 'done')
        frame = requestAnimationFrame(tick);
    } else canvas.remove();
    if ((!enabled() || document.hidden) && finishPromise) done();
  }
  desktop.addEventListener('change', sync);
  reduced.addEventListener('change', sync);
  document.addEventListener('visibilitychange', sync);
  window.addEventListener('resize', resize);
  window.addEventListener(
    'pagehide',
    () => {
      if (frame) cancelAnimationFrame(frame);
      done();
    },
    { once: true },
  );
  sync();
  return {
    finish() {
      if (!enabled() || document.hidden) return Promise.resolve();
      if (finishPromise) return finishPromise;
      canvas.dataset.phase = 'catch';
      finishTime = 0;
      finishPromise = new Promise((resolve) => (resolveFinish = resolve));
      // A completion animation must never hold navigation hostage.
      finishTimeout = setTimeout(done, 3800);
      return finishPromise;
    },
  };
}
