// Conservative drawing envelopes include hands, waving arms and the tilted card.
// Keeping their horizontal envelopes apart also guarantees separation in midair.
export const HUMAN_EXTENT = 40;
export const TASK_EXTENT = 90;
export const MIN_GAP = HUMAN_EXTENT + TASK_EXTENT + 14;
const chores = [
  'Sort receipts',
  'Fold laundry',
  'Take bins out',
  'File invoices',
  'Wipe counters',
  'Match socks',
  'Clear inbox',
  'Water plants',
  'Dust shelves',
  'Refill soap',
];
const mix = (a, b, t) => a + (b - a) * t;
const clamp = (value, min, max) => Math.max(min, Math.min(max, value));
const smooth = (t) => t * t * (3 - 2 * t);

// Kept independent of the canvas so wrap, spacing and offscreen finishes can be
// exercised at many frame rates without screenshots or animation test hooks.
export class ChaseMotion {
  constructor(width) {
    this.width = width;
    this.human = 65;
    this.gap = 220;
    this.time = this.modeTime = this.velocity = this.gait = this.jump = 0;
    this.jumpAge = 1;
    this.rest = this.finishing = this.gather = false;
    this.caughtAt = null;
    this.wraps = 0;
    this.direction = 1;
    this.flightAge = 0;
  }
  get task() {
    return this.human + this.gap;
  }
  get label() {
    return chores[this.wraps % chores.length];
  }
  get phase() {
    return this.caughtAt !== null
      ? 'celebrate'
      : this.finishing
        ? 'catch'
        : this.rest
          ? 'rest'
          : 'chase';
  }
  finish() {
    this.finishing = true;
    this.rest = false;
    // Turn around only after both racers have left the right edge. They then
    // return task-first, so the runner really pursues from the right, facing left.
    this.gather = this.task > this.width / 2;
    this.startFlight();
  }
  startFlight() {
    this.flightStart = { human: this.human, task: this.task };
    this.flightAge = 0;
  }
  get taskFlight() {
    return this.finishing ? smooth(clamp(this.flightAge / 1.8, 0, 1)) : 0;
  }
  get humanFlight() {
    return this.finishing ? smooth(clamp((this.flightAge - 0.16) / 1.94, 0, 1)) : 0;
  }
  advance(dt) {
    this.time += dt;
    this.modeTime += dt;
    if (!this.finishing) {
      if (this.modeTime > (this.rest ? 4 : 10)) {
        this.rest = !this.rest;
        this.modeTime = 0;
      }
      const speed = this.rest ? 0 : 320 + Math.sin(this.time * 0.8) * 20;
      this.velocity = mix(this.velocity, speed, 1 - Math.exp(-dt * 8));
      const previousTask = this.task;
      this.human += this.velocity * dt;
      const target = this.rest ? 210 : 196 + Math.sin(this.time * 1.3) * 47;
      this.gap = Math.max(
        MIN_GAP,
        previousTask - this.human,
        mix(this.gap, target, 1 - Math.exp(-dt * 4)),
      );
      if (!this.rest && this.gap < 161 && this.jumpAge > 0.72) this.jumpAge = 0;
      this.jumpAge += dt;
      this.jump =
        this.jumpAge < 0.72 ? Math.sin((this.jumpAge / 0.72) * Math.PI) * 45 : 0;
    } else {
      this.jump *= Math.exp(-dt * 12);
      if (this.gather) {
        this.velocity = mix(
          this.velocity,
          Math.max(600, this.width / 1.5),
          1 - Math.exp(-dt * 10),
        );
        this.human += this.velocity * dt;
        if (this.human > this.width + HUMAN_EXTENT + 12) {
          this.direction = -1;
          this.human = -this.gap - TASK_EXTENT - 24;
          this.gather = false;
          this.startFlight();
        }
      } else if (this.caughtAt === null) {
        this.flightAge += dt;
        // Land the whole pair centrally, with the card just ahead of its runner.
        const destination = this.width / 2 + (MIN_GAP + HUMAN_EXTENT - TASK_EXTENT) / 2;
        const task = mix(this.flightStart.task, destination, this.taskFlight);
        const previous = this.human;
        this.human = Math.min(
          task - MIN_GAP,
          mix(this.flightStart.human, destination - MIN_GAP, this.humanFlight),
        );
        this.gap = Math.max(MIN_GAP, task - this.human);
        this.velocity = Math.max(
          80,
          Math.abs(this.human - previous) / Math.max(dt, 0.001),
        );
        if (this.flightAge >= 2.1) {
          this.human = destination - MIN_GAP;
          this.gap = MIN_GAP;
          this.velocity = 0;
          this.caughtAt = this.time;
        }
      }
    }
    // The task exits first; only wrap once the runner has also completely left.
    // Both re-enter from the left in the same order, with exactly the same gap.
    if (!this.finishing && this.human > this.width + HUMAN_EXTENT + 12) {
      this.human = -this.gap - TASK_EXTENT - 24;
      this.wraps++;
    }
    const stride = 38 * clamp(this.velocity / 200, 0, 1);
    if (stride > 0.1) this.gait += ((this.velocity * dt * 0.38) / stride) * Math.PI * 2;
  }
}

export function welcomeChase() {
  const desktop = matchMedia(
    '(min-width: 1024px) and (pointer: fine), (min-width: 1024px) and (pointer: none)',
  );
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  const canvas = document.createElement('canvas');
  const ctx = canvas.getContext('2d');
  if (!ctx) return { finish: () => Promise.resolve(), remove() {} };
  canvas.className = 'welcome-chase';
  canvas.setAttribute('aria-hidden', 'true');
  canvas.dataset.phase = 'chase';
  canvas.dataset.complete = 'false';
  const motion = new ChaseMotion(innerWidth);
  const ink = '#aab6c7';
  let frame,
    last,
    resolveFinish,
    finishPromise,
    finishTimeout,
    finishHold,
    particles = [],
    height = 128,
    landingLift = 0;
  let disposed = false;
  const enabled = () => !disposed && desktop.matches && !reduced.matches;
  function resize() {
    // Use the rendered width so a page scrollbar cannot squash thin strokes.
    motion.width = canvas.getBoundingClientRect().width || innerWidth;
    // Supersample thin moving limbs even on standard-density desktop screens.
    const ratio = Math.max(2, Math.min(devicePixelRatio || 1, 3));
    canvas.width = Math.round(motion.width * ratio);
    height = motion.finishing ? innerHeight : 128;
    canvas.style.height = `${height}px`;
    canvas.height = Math.round(height * ratio);
    const message = document.querySelector('[data-welcome-complete]');
    const messageTop = message?.getBoundingClientRect().top || innerHeight * 0.45;
    // Leave a calm gap between the completed card and the welcome message.
    landingLift = Math.max(0, innerHeight - messageTop + 64);
    ctx.setTransform(canvas.width / motion.width, 0, 0, ratio, 0, 0);
  }
  function ellipse(x, y, rx, ry, fill) {
    ctx.beginPath();
    ctx.ellipse(x, y, rx, ry, 0, 0, Math.PI * 2);
    ctx.fillStyle = fill;
    ctx.fill();
  }
  function capsule(a, b, radius) {
    ctx.lineWidth = radius * 2;
    ctx.lineCap = 'round';
    ctx.strokeStyle = ink;
    ctx.beginPath();
    ctx.moveTo(...a);
    ctx.lineTo(...b);
    ctx.stroke();
  }
  function knee(hip, foot) {
    const dx = foot[0] - hip[0],
      dy = foot[1] - hip[1];
    const distance = Math.hypot(dx, dy) || 1;
    const h = Math.sqrt(Math.max(0, 15.5 ** 2 - Math.min(distance, 30.9) ** 2 / 4));
    return [
      (hip[0] + foot[0]) / 2 - (dy / distance) * h,
      (hip[1] + foot[1]) / 2 + (dx / distance) * h,
    ];
  }
  function drawHuman(celebration) {
    const { human, velocity, gait, time, rest, finishing, gap } = motion;
    const running = clamp(velocity / 200, 0, 1);
    const breath = Math.sin(time * 2.8) * (1 - running) * 0.65;
    const bob = running * (1 - Math.cos(gait * 2)) * 1.4;
    const joy = celebration
      ? Math.max(0, Math.sin(celebration * 10)) * 9 * Math.max(0, 1 - celebration / 1.5)
      : 0;
    const lean = running * (7 + Math.sin(gait) * 0.7) + (rest ? 2 : 0);
    const p = (x, y, z = 0) => [human + x + z * 0.22, 125 - y - joy + z * 0.1];
    ellipse(human, 124, 17 - joy * 0.2, 2.3, '#00000025');
    function leg(offset, depth) {
      const phase = (((gait / (Math.PI * 2) + offset) % 1) + 1) % 1;
      const stance = 0.38,
        stride = 38 * running;
      const swing = (phase - stance) / (1 - stance);
      const x =
        phase < stance ? stride * (0.5 - phase / stance) : stride * (smooth(swing) - 0.5);
      const lift = phase < stance ? 0 : Math.sin(swing * Math.PI) ** 1.3 * 19 * running;
      const hip = [0, 27 + bob + breath],
        foot = [x + (1 - running) * (offset ? -4 : 4), 3 + lift];
      const joint = knee(hip, foot);
      capsule(p(...hip, depth), p(...joint, depth), 1.9);
      capsule(p(...joint, depth), p(...foot, depth), 1.7);
      capsule(
        p(foot[0] - 1, foot[1] - 0.4, depth),
        p(foot[0] + 5, foot[1] - 0.4, depth),
        1.8,
      );
    }
    function arm(offset, depth) {
      // Oppose the leg, with a relaxed bent elbow rather than a rigid pendulum.
      const angle = -Math.cos(gait + offset * Math.PI * 2) * 0.85 * running;
      const shoulder = [lean, 46 + bob + breath];
      let elbow = [
        shoulder[0] + Math.sin(angle) * 12,
        shoulder[1] - Math.cos(angle) * 12,
      ];
      let hand = [
        elbow[0] + Math.cos(angle + 0.4) * 10,
        elbow[1] + Math.sin(angle + 0.4) * 10,
      ];
      if (rest && running < 0.1) {
        elbow = [7, 36 + breath];
        hand = [4, 28 + breath];
      }
      if (finishing && !celebration && gap < MIN_GAP + 30) {
        elbow = [18, 39];
        hand = [34, 43];
      }
      if (celebration) {
        elbow = [offset ? -10 : 12, 57];
        hand = [offset ? -14 : 17, 67 + Math.sin(celebration * 12 + offset) * 2];
      }
      capsule(p(...shoulder, depth), p(...elbow, depth), 1.7);
      capsule(p(...elbow, depth), p(...hand, depth), 1.5);
    }
    arm(0.5, -4);
    leg(0.5, -3);
    capsule(p(0, 28 + breath), p(lean, 45 + bob + breath), 3);
    leg(0, 3);
    capsule(p(lean, 47 + bob + breath), p(lean + 1, 52 + bob + breath), 1.8);
    const head = p(lean + 1, 59 + bob + breath);
    ellipse(...head, 5.5, 5.5, ink);
    arm(0, 4);
    if (rest && running < 0.1 && !celebration) {
      const puff = (time * 0.65) % 1;
      ellipse(
        head[0] + 12 + puff * 11,
        head[1] + 3 - puff * 6,
        2 + puff * 2,
        1.2,
        `rgba(148,163,184,${0.3 * (1 - puff)})`,
      );
    }
  }
  function drawTask(celebration) {
    const { rest, time, jump, jumpAge, caughtAt } = motion;
    const task = motion.direction === -1 ? motion.width - motion.task : motion.task;
    const checked = caughtAt !== null;
    const bounce = rest && !checked ? Math.abs(Math.sin(time * 5)) * 7 : 0;
    const y =
      102 -
      jump -
      bounce -
      (celebration ? Math.sin(Math.min(1, celebration) * Math.PI) * 6 : 0);
    ellipse(task, 124, 34 - jump * 0.2, 2.3, `rgba(0,0,0,${0.16 - jump * 0.002})`);
    ctx.save();
    ctx.translate(task, y);
    ctx.rotate(
      rest && !checked
        ? Math.sin(time * 5) * 0.07
        : jump > 0
          ? -Math.sin((jumpAge / 0.72) * Math.PI) * 0.08
          : 0.012 * Math.sin(time * 3),
    );
    ctx.fillStyle = checked ? '#52a98b' : '#8995a8';
    ctx.beginPath();
    ctx.roundRect(-77, -14, 156, 32, 6);
    ctx.fill();
    const paper = ctx.createLinearGradient(0, -17, 0, 15);
    paper.addColorStop(0, checked ? '#d1fae5' : '#f1f5f9');
    paper.addColorStop(1, checked ? '#a7e6cb' : '#dce3ed');
    ctx.fillStyle = paper;
    ctx.beginPath();
    ctx.roundRect(-79, -17, 156, 32, 6);
    ctx.fill();
    ctx.strokeStyle = checked ? '#268664' : '#8291a6';
    ctx.lineWidth = 1.4;
    ctx.beginPath();
    ctx.roundRect(-67, -7, 12, 12, 3);
    ctx.stroke();
    if (checked) {
      ctx.strokeStyle = '#21805d';
      ctx.lineWidth = 2;
      ctx.lineJoin = 'round';
      ctx.beginPath();
      ctx.moveTo(-64, -1);
      ctx.lineTo(-61, 2);
      ctx.lineTo(-57, -4);
      ctx.stroke();
    }
    ctx.font = '500 12px system-ui, sans-serif';
    ctx.textBaseline = 'middle';
    ctx.fillStyle = checked ? '#246a53' : '#435166';
    ctx.fillText(motion.label, -45, -0.5);
    if (checked) {
      ctx.strokeStyle = '#438b71';
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(-45, 0);
      ctx.lineTo(-45 + ctx.measureText(motion.label).width, 0);
      ctx.stroke();
    }
    if (rest || checked) {
      for (const side of [-1, 1]) {
        ctx.strokeStyle = checked ? '#79b69c' : '#a7b4c8';
        ctx.lineWidth = 1.8;
        ctx.lineCap = 'round';
        ctx.beginPath();
        ctx.moveTo(side * 78, -2);
        ctx.quadraticCurveTo(
          side * 84,
          -8,
          side * 88,
          -12 + Math.sin(time * 9 + side) * 3,
        );
        ctx.stroke();
      }
    }
    ctx.restore();
  }
  function confetti() {
    particles = Array.from({ length: 24 }, (_, i) => ({
      x: motion.direction === -1 ? motion.width - motion.task : motion.task,
      y: 87,
      vx: Math.cos(i * 2.4) * (35 + (i % 5) * 15),
      vy: -65 - (i % 7) * 15,
      age: 0,
      color: ['#94a3b8', '#cbd5e1', '#6ee7b7'][i % 3],
    }));
  }
  function done() {
    clearTimeout(finishTimeout);
    clearTimeout(finishHold);
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
    const wasCaught = motion.caughtAt !== null;
    motion.advance(dt);
    if (!wasCaught && motion.caughtAt !== null) {
      canvas.dataset.complete = 'true';
      confetti();
    }
    canvas.dataset.phase = motion.phase;
    ctx.clearRect(0, 0, motion.width, height);
    const celebration =
      motion.caughtAt === null ? 0 : motion.time - motion.caughtAt + 0.001;
    ctx.save();
    ctx.translate(0, height - 128 - landingLift * motion.humanFlight);
    if (motion.direction === -1) {
      ctx.translate(motion.width, 0);
      ctx.scale(-1, 1);
    }
    drawHuman(celebration);
    ctx.restore();
    ctx.save();
    ctx.translate(0, height - 128 - landingLift * motion.taskFlight);
    drawTask(celebration);
    ctx.restore();
    ctx.save();
    ctx.translate(0, height - 128 - landingLift);
    for (const particle of particles) {
      particle.age += dt;
      particle.vy += 130 * dt;
      particle.x += particle.vx * dt;
      particle.y += particle.vy * dt;
      ctx.globalAlpha = Math.max(0, 1 - particle.age / 1.5);
      ctx.fillStyle = particle.color;
      ctx.save();
      ctx.translate(particle.x, particle.y);
      ctx.rotate(particle.age * 5);
      ctx.fillRect(-2, -1, 4, 2);
      ctx.restore();
    }
    ctx.restore();
    ctx.globalAlpha = 1;
    if (celebration > 1.7) {
      // Freeze the finished scene, leaving time to read before opening the app.
      canvas.dataset.phase = 'settled';
      finishHold = setTimeout(done, 3000);
      return;
    }
    if (canvas.dataset.phase !== 'done') frame = requestAnimationFrame(tick);
  }
  function sync() {
    if (frame) cancelAnimationFrame(frame);
    frame = null;
    last = null;
    if (enabled()) {
      if (!canvas.isConnected) document.body.append(canvas);
      resize();
      if (!document.hidden && !['done', 'settled'].includes(canvas.dataset.phase))
        frame = requestAnimationFrame(tick);
    } else canvas.remove();
    if ((!enabled() || document.hidden) && finishPromise) done();
  }
  desktop.addEventListener('change', sync);
  reduced.addEventListener('change', sync);
  document.addEventListener('visibilitychange', sync);
  window.addEventListener('resize', resize);
  function remove() {
    disposed = true;
    if (frame) cancelAnimationFrame(frame);
    canvas.remove();
    desktop.removeEventListener('change', sync);
    reduced.removeEventListener('change', sync);
    document.removeEventListener('visibilitychange', sync);
    window.removeEventListener('resize', resize);
    window.removeEventListener('pagehide', remove);
    done();
  }
  window.addEventListener('pagehide', remove, { once: true });
  sync();
  return {
    remove,
    finish() {
      if (!enabled() || document.hidden) return Promise.resolve();
      if (finishPromise) return finishPromise;
      motion.finish();
      resize();
      canvas.dataset.phase = 'catch';
      finishPromise = new Promise((resolve) => (resolveFinish = resolve));
      finishTimeout = setTimeout(done, 9000);
      return finishPromise;
    },
  };
}
