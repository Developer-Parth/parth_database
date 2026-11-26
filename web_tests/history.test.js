var assert = require("assert");
var history = require("../history.js");

assert.deepStrictEqual(history.pushHistory([], "  SELECT 1  "), ["SELECT 1"]);
assert.deepStrictEqual(history.pushHistory(["a", "b"], "c"), ["c", "a", "b"]);
assert.deepStrictEqual(history.pushHistory(["a", "b", "c"], "b"), ["b", "a", "c"]);
assert.deepStrictEqual(history.pushHistory(["a"], "   "), ["a"]);

var long = [];
for (var i = 0; i < 40; i++) { long = history.pushHistory(long, "q" + i); }
assert.strictEqual(long.length, history.HISTORY_LIMIT);
assert.strictEqual(long[0], "q39");

function fakeStorage(initial) {
  var data = initial === undefined ? {} : { parth_db_history: initial };
  return {
    getItem: function (key) { return key in data ? data[key] : null; },
    setItem: function (key, value) { data[key] = value; }
  };
}

assert.deepStrictEqual(history.loadHistory(fakeStorage()), []);
assert.deepStrictEqual(history.loadHistory(fakeStorage("not json")), []);
assert.deepStrictEqual(history.loadHistory(fakeStorage("{}")), []);
assert.deepStrictEqual(history.loadHistory(fakeStorage('["a", 3, "", "b"]')), ["a", "b"]);
assert.deepStrictEqual(history.loadHistory(null), []);

var store = fakeStorage();
assert.strictEqual(history.saveHistory(store, ["x", "y"]), true);
assert.deepStrictEqual(history.loadHistory(store), ["x", "y"]);
assert.strictEqual(history.saveHistory(null, ["x"]), false);

console.log("history tests passed");
