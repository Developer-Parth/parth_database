var assert = require("assert");
var h = require("../history.js");

function fakeStorage() {
  var data = {};
  return {
    getItem: function (k) { return Object.prototype.hasOwnProperty.call(data, k) ? data[k] : null; },
    setItem: function (k, v) { data[k] = String(v); },
    removeItem: function (k) { delete data[k]; },
    key: function (i) { return Object.keys(data)[i] || null; },
    get length() { return Object.keys(data).length; },
    raw: data
  };
}

var list = h.pushHistory([], "  SELECT 1  ");
assert.deepStrictEqual(list, ["SELECT 1"]);
list = h.pushHistory(list, "SELECT 2");
list = h.pushHistory(list, "SELECT 1");
assert.deepStrictEqual(list, ["SELECT 1", "SELECT 2"]);
assert.deepStrictEqual(h.pushHistory(list, "   "), list);

var big = [];
for (var i = 0; i < 40; i++) { big = h.pushHistory(big, "SELECT " + i); }
assert.strictEqual(big.length, h.HISTORY_LIMIT);
assert.strictEqual(big[0], "SELECT 39");

var s = fakeStorage();
assert.strictEqual(h.isRemembering(s), false);
h.saveHistory(s, "http://a", ["SELECT a"]);
h.saveHistory(s, "http://b", ["SELECT b"]);
assert.deepStrictEqual(h.loadHistory(s, "http://a"), ["SELECT a"]);
assert.deepStrictEqual(h.loadHistory(s, "http://b"), ["SELECT b"]);
assert.deepStrictEqual(h.loadHistory(s, "http://c"), []);

h.saveHistory(s, "http://a", []);
assert.strictEqual(s.getItem("parth_db_history:http://a"), null);
assert.deepStrictEqual(h.loadHistory(s, "http://b"), ["SELECT b"]);

h.setRemembering(s, true);
assert.strictEqual(h.isRemembering(s), true);
h.forgetAll(s);
assert.deepStrictEqual(h.loadHistory(s, "http://b"), []);
assert.strictEqual(h.isRemembering(s), true);
h.setRemembering(s, false);
assert.strictEqual(h.isRemembering(s), false);

var bad = fakeStorage();
bad.setItem("parth_db_history:http://a", "{not json");
assert.deepStrictEqual(h.loadHistory(bad, "http://a"), []);
bad.setItem("parth_db_history:http://a", JSON.stringify({ a: 1 }));
assert.deepStrictEqual(h.loadHistory(bad, "http://a"), []);
bad.setItem("parth_db_history:http://a", JSON.stringify(["ok", 5, "", null, "fine"]));
assert.deepStrictEqual(h.loadHistory(bad, "http://a"), ["ok", "fine"]);

assert.deepStrictEqual(h.loadHistory(null, "http://a"), []);
assert.strictEqual(h.saveHistory(null, "http://a", ["x"]), false);
assert.strictEqual(h.isRemembering(null), false);
assert.strictEqual(h.forgetAll(null), false);

var throwing = {
  getItem: function () { throw new Error("blocked"); },
  setItem: function () { throw new Error("quota"); },
  removeItem: function () { throw new Error("blocked"); },
  key: function () { throw new Error("blocked"); },
  length: 1
};
assert.deepStrictEqual(h.loadHistory(throwing, "http://a"), []);
assert.strictEqual(h.saveHistory(throwing, "http://a", ["x"]), false);
assert.strictEqual(h.isRemembering(throwing), false);
assert.strictEqual(h.setRemembering(throwing, true), false);
assert.strictEqual(h.forgetAll(throwing), false);

console.log("history tests passed");
