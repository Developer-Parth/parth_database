var assert = require("assert");
var createLatest = require("../latest.js").createLatest;
var runGuarded = require("../guarded.js").runGuarded;

function deferred() {
  var d = {};
  d.promise = new Promise(function (resolve, reject) { d.resolve = resolve; d.reject = reject; });
  return d;
}

function settle() { return new Promise(function (resolve) { setImmediate(resolve); }); }

async function run() {
  var latest = createLatest();
  var shown = [];
  var errors = [];
  function ok(v) { shown.push(v); }
  function fail(e) { errors.push(e); }

  var first = deferred();
  var second = deferred();
  runGuarded(latest, first.promise, ok, fail);
  runGuarded(latest, second.promise, ok, fail);
  second.resolve("second");
  await settle();
  first.resolve("first");
  await settle();
  assert.deepStrictEqual(shown, ["second"]);

  var slow = deferred();
  var fast = deferred();
  runGuarded(latest, slow.promise, ok, fail);
  runGuarded(latest, fast.promise, ok, fail);
  fast.resolve("fast");
  await settle();
  slow.reject(new Error("late failure"));
  await settle();
  assert.deepStrictEqual(shown, ["second", "fast"]);
  assert.strictEqual(errors.length, 0);

  var current = deferred();
  runGuarded(latest, current.promise, ok, fail);
  current.reject(new Error("real failure"));
  await settle();
  assert.strictEqual(errors.length, 1);
  assert.strictEqual(errors[0].message, "real failure");

  var beforeSwitch = deferred();
  var beforeSwitchFail = deferred();
  runGuarded(latest, beforeSwitch.promise, ok, fail);
  runGuarded(latest, beforeSwitchFail.promise, ok, fail);
  latest.invalidate();
  beforeSwitch.resolve("old server result");
  beforeSwitchFail.reject(new Error("old server error"));
  await settle();
  assert.deepStrictEqual(shown, ["second", "fast"]);
  assert.strictEqual(errors.length, 1);

  var after = deferred();
  runGuarded(latest, after.promise, ok, fail);
  after.resolve("new server result");
  await settle();
  assert.deepStrictEqual(shown, ["second", "fast", "new server result"]);
}

run().then(function () { console.log("guarded tests passed"); }, function (e) { console.error(e); process.exit(1); });
