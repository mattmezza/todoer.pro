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
