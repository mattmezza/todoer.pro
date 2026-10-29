import test from 'node:test';
import assert from 'node:assert/strict';
import {
  ChaseMotion,
  HUMAN_EXTENT,
  TASK_EXTENT,
  MIN_GAP,
} from '../assets/welcome-chase.js';
function separate(motion) {
  assert.ok(
    motion.task - TASK_EXTENT > motion.human + HUMAN_EXTENT,
    `drawing envelopes overlap at ${motion.time}s`,
  );
  assert.ok(motion.gap >= MIN_GAP);
}
test('fast one-way passes wrap only once both actors are offscreen; rest and near misses stay apart', () => {
  for (const width of [1024, 1280, 1920, 2560])
    for (const fps of [25, 60, 120]) {
      const motion = new ChaseMotion(width);
      let rests = 0,
        jumps = 0,
        previousPhase = motion.phase;
      for (let i = 0; i < fps * 180; i++) {
        const before = { human: motion.human, task: motion.task, wraps: motion.wraps };
        motion.advance(1 / fps);
        separate(motion);
        assert.equal(motion.caughtAt, null);
        if (motion.wraps !== before.wraps) {
          assert.ok(before.human > width + HUMAN_EXTENT - 12);
          assert.ok(before.task - TASK_EXTENT > width);
          assert.ok(motion.task + TASK_EXTENT < 0);
        } else {
          assert.ok(motion.human >= before.human, 'runner unexpectedly reversed');
          assert.ok(
            motion.task >= before.task - 1e-9,
            'task should not peek back through its exit',
          );
        }
        if (motion.phase === 'rest' && previousPhase !== 'rest') rests++;
        if (motion.jump > 0) jumps++;
        previousPhase = motion.phase;
      }
      assert.ok(motion.wraps > 5);
      assert.ok(rests > 5);
      assert.ok(jumps > 0);
      assert.ok(
        motion.time / motion.wraps < 22,
        'passes should be brisk, even with rests',
      );
    }
});
test('completion from any point of a pass returns on-screen and never overlaps', () => {
  for (const width of [1024, 1280, 1920, 2560])
    for (const start of [0, 1, 4, 5, 6, 7, 8, 9, 10, 12, 14, 16, 20, 28]) {
      const motion = new ChaseMotion(width);
      for (let i = 0; i < start * 60; i++) motion.advance(1 / 60);
      const fromRight = motion.task > width / 2;
      motion.finish();
      for (let i = 0; i < 60 * 4 && motion.caughtAt === null; i++) {
        motion.advance(1 / 60);
        separate(motion);
        assert.ok(motion.taskFlight >= motion.humanFlight);
      }
      assert.notEqual(
        motion.caughtAt,
        null,
        `finish took too long: width ${width}, start ${start}`,
      );
      assert.equal(motion.direction, fromRight ? -1 : 1);
      assert.ok(Math.abs((motion.human - HUMAN_EXTENT + motion.task + TASK_EXTENT) / 2 - width / 2) < 0.01);
      assert.equal(motion.taskFlight, 1);
      assert.equal(motion.humanFlight, 1);
      assert.ok(motion.human - HUMAN_EXTENT >= 0);
      assert.ok(motion.task + TASK_EXTENT <= width);
      for (let i = 0; i < 120; i++) {
        motion.advance(1 / 60);
        separate(motion);
      }
    }
});
test('resizing while offscreen preserves the gap and permits a visible finish', () => {
  const motion = new ChaseMotion(2560);
  for (let i = 0; i < 600; i++) motion.advance(1 / 60);
  motion.width = 1024;
  for (let i = 0; i < 60; i++) {
    motion.advance(1 / 60);
    separate(motion);
  }
  motion.finish();
  for (let i = 0; i < 240 && motion.caughtAt === null; i++) {
    motion.advance(1 / 60);
    separate(motion);
  }
  assert.notEqual(motion.caughtAt, null);
});

test('each offscreen wrap brings back a different chore, stable during each pass', () => {
  const motion = new ChaseMotion(1280);
  let labels = new Set([motion.label]);
  let wraps = 0;
  for (let i = 0; i < 60 * 120; i++) {
    const previous = motion.label;
    motion.advance(1 / 60);
    if (motion.wraps !== wraps) {
      assert.notEqual(motion.label, previous);
      assert.ok(motion.task + TASK_EXTENT < 0, 'label changed while card was visible');
      labels.add(motion.label);
      wraps = motion.wraps;
    } else assert.equal(motion.label, previous);
  }
  assert.ok(labels.size >= 8);
});
