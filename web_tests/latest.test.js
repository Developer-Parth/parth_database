var assert = require("assert");
var createLatest = require("../latest.js").createLatest;

function deferred() {
  var d = {};
  d.promise = new Promise(function (resolve, reject) {
    d.resolve = resolve;
    d.reject = reject;
  });
  return d;
}

function load(latest, d, shown, errors) {
  var id = latest.begin();
  return d.promise.then(function (value) {
    if (latest.isCurrent(id)) {
      shown.push(value);
    }
  }, function (error) {
    if (latest.isCurrent(id)) {
      errors.push(error.message);
    }
  });
}

var latest = createLatest();
var shown = [];
var errors = [];

var a = deferred();
var b = deferred();
var pa = load(latest, a, shown, errors);
var pb = load(latest, b, shown, errors);
b.resolve("second");
a.resolve("first-late");

Promise.all([pa, pb]).then(function () {
  assert.deepStrictEqual(shown, ["second"]);
  var c = deferred();
  var d = deferred();
  var pc = load(latest, c, shown, errors);
  var pd = load(latest, d, shown, errors);
  d.resolve("fourth");
  c.reject(new Error("stale failure"));
  return Promise.all([pc, pd]);
}).then(function () {
  assert.deepStrictEqual(shown, ["second", "fourth"]);
  assert.deepStrictEqual(errors, []);
  var e = deferred();
  var pe = load(latest, e, shown, errors);
  e.reject(new Error("current failure"));
  return pe;
}).then(function () {
  assert.deepStrictEqual(errors, ["current failure"]);
  var f = deferred();
  var pf = load(latest, f, shown, errors);
  latest.invalidate();
  f.resolve("old server rows");
  return pf;
}).then(function () {
  assert.deepStrictEqual(shown, ["second", "fourth"]);
  var g = deferred();
  var pg = load(latest, g, shown, errors);
  latest.invalidate();
  g.reject(new Error("old server failure"));
  return pg;
}).then(function () {
  assert.deepStrictEqual(errors, ["current failure"]);
  console.log("latest tests passed");
});
