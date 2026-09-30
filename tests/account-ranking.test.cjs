const test = require('node:test');
const assert = require('node:assert/strict');
const { rankAccounts, getRemainingQuota } = require('../public/account-ranking.js');

function account(id, five, weekly, extra = {}) {
  return {
    id, email: `${id}@example.test`,
    usage: {
      primary: five == null ? null : { usedPercent: 100 - five },
      secondary: weekly == null ? null : { usedPercent: 100 - weekly },
      ...extra,
    },
  };
}
const ids = accounts => rankAccounts(accounts).map(a => a.id);

test('either exhausted window goes below usable accounts, even with reset credits', () => {
  assert.deepEqual(ids([
    account('weekly-empty', 100, 0, { resetCredits: 10 }),
    account('five-empty', 0, 100),
    account('usable', 20, 30),
  ]).slice(0, 1), ['usable']);
});

test('the smaller remaining quota controls ranking before five-hour headroom', () => {
  assert.deepEqual(ids([account('low-weekly', 100, 13), account('balanced', 90, 52)]), ['balanced', 'low-weekly']);
  assert.deepEqual(ids([account('a', 100, 73), account('b', 100, 57), account('c', 90, 52)]), ['a', 'b', 'c']);
});

test('equal bottlenecks prefer more five-hour headroom', () => {
  assert.deepEqual(ids([account('less-five', 50, 90), account('more-five', 90, 50)]), ['more-five', 'less-five']);
});

test('an explicit limitReached flag cannot be overridden by positive percentages', () => {
  assert.deepEqual(ids([account('blocked', 100, 100, { limitReached: true }), account('usable', 1, 1)]), ['usable', 'blocked']);
});

test('missing usage is not treated as full quota and errors come last', () => {
  assert.deepEqual(ids([
    account('error', 100, 100, { error: 'Status 401' }),
    account('unknown-weekly', 100, null),
    account('blocked', 100, 0),
    account('usable', 1, 1),
  ]), ['usable', 'unknown-weekly', 'blocked', 'error']);
});

test('unknown and invalid percentage values stay unknown', () => {
  for (const value of [undefined, null, '0', NaN, Infinity]) {
    assert.equal(getRemainingQuota({ usedPercent: value }), null);
  }
  assert.equal(getRemainingQuota({ usedPercent: -5 }), 100);
  assert.equal(getRemainingQuota({ usedPercent: 150 }), 0);
});

test('blocked accounts prefer the time when all exhausted windows will reset', () => {
  const one = account('one-window', 0, 50);
  one.usage.primary.resetAfterSeconds = 200;
  const two = account('two-windows', 0, 0);
  two.usage.primary.resetAfterSeconds = 10;
  two.usage.secondary.resetAfterSeconds = 300;
  assert.deepEqual(ids([two, one]), ['one-window', 'two-windows']);
});

test('exact quota ties use credits then active state, without pinning a worse active account', () => {
  const active = { ...account('active', 50, 50), isActive: true };
  assert.deepEqual(ids([account('plain', 50, 50), active]), ['active', 'plain']);
  assert.deepEqual(ids([active, account('credits', 50, 50, { resetCredits: 2 })]), ['credits', 'active']);
  assert.deepEqual(ids([active, account('better', 60, 60)]), ['better', 'active']);
});

test('ranking preserves source order and has a deterministic final tie breaker', () => {
  const source = [account('b', 50, 50), account('a', 50, 50)];
  assert.deepEqual(ids(source), ['a', 'b']);
  assert.equal(source[0].id, 'b');
});
