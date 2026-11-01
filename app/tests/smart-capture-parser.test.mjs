import test from 'node:test';
import assert from 'node:assert/strict';
import { parseCapture } from '../assets/smart-capture-parser.mjs';
const destinations = [
  {
    space: 'personal',
    space_id: 'a',
    list: 'groceries',
    list_id: '1',
    default_list: true,
  },
  {
    space: 'work space',
    space_id: 'b',
    list: 'groceries',
    list_id: '2',
    default_list: true,
  },
];
const parse = (text) => parseCapture(text, { destinations, now: new Date(2026, 9, 28) });
test('extracts adjacent destination and metadata', () => {
  const p = parse('buy milk @personal#groceries today p0 +food');
  assert.equal(p.title, 'buy milk');
  assert.equal(p.destination.list_id, '1');
  assert.equal(p.due, '2026-10-28');
  assert.equal(p.priority, 0);
  assert.deepEqual(p.tags, ['food']);
});
test('ambiguous and unknown destinations remain literal', () => {
  for (const input of ['buy #groceries', 'buy @missing#groceries']) {
    const p = parse(input);
    assert.equal(p.title, input);
    assert.equal(p.destination, null);
    assert.ok(p.warnings.length);
  }
});
test('quoted names, priority and local relative dates', () => {
  const p = parse('work @"work space"#groceries in 3 days !!!');
  assert.equal(p.destination.list_id, '2');
  assert.equal(p.priority, 0);
  assert.equal(p.due, '2026-10-31');
  assert.equal(p.title, 'work');
});
test('emails and conflicting priority are preserved', () => {
  const input = 'email me@example.com p1 p2';
  const p = parse(input);
  assert.equal(p.title, input);
  assert.equal(p.priority, null);
});
test('date parsing can be disabled', () =>
  assert.equal(
    parseCapture('report tomorrow', { dates: false }).title,
    'report tomorrow',
  ));

test('date-like words inside quoted tags remain part of the tag', () => {
  const p = parse('read +"today notes" tomorrow');
  assert.equal(p.title, 'read');
  assert.deepEqual(p.tags, ['today notes']);
  assert.equal(p.due, '2026-10-29');
});

test('planned and due prefixes share aliases and can be combined independently', () => {
  const expected = new Map([
    ['today', '2026-10-28'], ['t', '2026-10-28'], ['td','2026-10-28'],
    ['tomo','2026-10-29'], ['tomorrow','2026-10-29'], ['tmr','2026-10-29'],
    ['mon','2026-11-02'], ['monday','2026-11-02'], ['tue','2026-11-03'], ['tuesday','2026-11-03'],
    ['wed','2026-11-04'], ['wednesday','2026-11-04'], ['thu','2026-10-29'], ['thursday','2026-10-29'],
    ['fri','2026-10-30'], ['friday','2026-10-30'], ['sat','2026-10-31'], ['saturday','2026-10-31'], ['sun','2026-11-01'], ['sunday','2026-11-01'],
    ['oct31','2026-10-31'], ['31oct','2026-10-31'], ['october31','2026-10-31'],
    ['nw','2026-11-02'], ['nextweek','2026-11-02'], ['nm','2026-11-01'], ['nextmonth','2026-11-01'],
    ['in1week','2026-11-04'], ['in1month','2026-11-28'], ['in3days','2026-10-31'], ['in3d','2026-10-31'], ['in2w','2026-11-11'], ['in2m','2026-12-28'],
    ['2027-01-01','2027-01-01'], ['weekend','2026-10-31'], ['feb29','2028-02-29'],
  ]);
  for (const [alias, date] of expected) for (const [prefix, kind] of [['.','planned'],['-','due']]) {
    const result = parse(`buy milk ${prefix}${alias} p1 +family`);
    assert.equal(result[kind], date, prefix+alias);
    assert.equal(result.title,'buy milk');
    assert.equal(result.priority,1); assert.deepEqual(result.tags,['family']);
  }
  const both = parse('buy milk .tomo -nw');
  assert.equal(both.planned,'2026-10-29'); assert.equal(both.due,'2026-11-02'); assert.equal(both.title,'buy milk');
});
test('calendar offsets clamp months and retain local dates across DST', () => {
  assert.equal(parseCapture('task .in1m', {now:new Date(2027,0,31)}).planned,'2027-02-28');
  assert.equal(parseCapture('task .in1y', {now:new Date(2028,1,29)}).planned,'2029-02-28');
  assert.equal(parseCapture('task .in3d', {now:new Date(2026,2,27)}).planned,'2026-03-30');
  assert.equal(parseCapture('task .oct31', {now:new Date(2026,10,1)}).planned,'2027-10-31');
});
test('unknown, impossible, quoted and conflicting date shortcuts remain literal', () => {
  for(const token of ['.feb30','-feb30','.in99999d','-in0d','.2026-02-29','.nonsense']) assert.equal(parse(`task ${token}`).title,`task ${token}`);
  const conflict=parse('task .today .tomo -nw');
  assert.equal(conflict.planned,null);assert.equal(conflict.due,'2026-11-02');assert.equal(conflict.title,'task .today .tomo');
  const dueConflict=parse('task -today -tomo .nw');
  assert.equal(dueConflict.due,null);assert.equal(dueConflict.planned,'2026-11-02');
  const quoted=parse('task +".today -tomo"');
  assert.equal(quoted.planned,null);assert.equal(quoted.due,null);assert.deepEqual(quoted.tags,['.today -tomo']);
  assert.equal(parseCapture('task .tomo -nw',{dates:false}).title,'task .tomo -nw');
  const scoped=parseCapture('task tomorrow .tomo -nw',{naturalDates:false,now:new Date(2026,9,28)});
  assert.equal(scoped.title,'task tomorrow');assert.equal(scoped.planned,'2026-10-29');assert.equal(scoped.due,'2026-11-02');
});
test('date shortcuts without a title never silently create an empty task',()=>{
  const result=parse('.tomo -nw');assert.equal(result.title,'.tomo -nw');assert.equal(result.planned,null);assert.equal(result.due,null);
});

test('explicit duration shortcuts preserve zero, long units and compound intervals', () => {
  for (const [token, minutes] of [
    [':0', 0],
    [':0m', 0],
    [':15m', 15],
    [':15min', 15],
    [':15minutes', 15],
    [':2h', 120],
    [':2 hours', 120],
    [':2hr', 120],
    [':2d', 2880],
    [':2days', 2880],
    [':1d2h30m', 1590],
    [':1 hour 30 minutes', 90],
  ]) {
    const result = parse(`Plan task ${token}`);
    assert.equal(result.title, 'Plan task', token);
    assert.equal(result.duration, minutes, token);
  }
  for (const title of [
    'Meet at 12:30',
    'https://example.test/x',
    'Task :10seconds',
    'Task :abc',
    'Task "keep :2h as text"',
  ]) {
    assert.equal(parse(title).title, title);
    assert.equal(parse(title).duration, null);
  }
  const combined = parse('Task .tomo :2h p1');
  assert.equal(combined.title, 'Task');
  assert.equal(combined.planned, '2026-10-29');
  assert.equal(combined.duration, 120);
  assert.equal(combined.priority, 1);
  const conflicting = parse('Task :1h :2h');
  assert.equal(conflicting.duration, null);
  assert.equal(conflicting.title, 'Task :1h :2h');
  assert.ok(
    conflicting.warnings.some((warning) => warning.includes('Multiple durations')),
  );
  const tooLong = parse('Task :4000days');
  assert.equal(tooLong.duration, null);
  assert.equal(tooLong.title, 'Task :4000days');
  assert.ok(tooLong.warnings.some((warning) => warning.includes('supported limit')));
  assert.equal(parse(':0').duration, null);
});
