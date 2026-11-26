var assert = require("assert");
var p = require("../paging.js");

assert.strictEqual(p.pageCount(0, 100), 1);
assert.strictEqual(p.pageCount(1, 100), 1);
assert.strictEqual(p.pageCount(100, 100), 1);
assert.strictEqual(p.pageCount(101, 100), 2);
assert.strictEqual(p.pageCount(1000, 100), 10);

assert.strictEqual(p.pageOffset(1, 100), 0);
assert.strictEqual(p.pageOffset(3, 100), 200);

assert.strictEqual(p.clampPage(9, 250, 100), 3);
assert.strictEqual(p.clampPage(0, 250, 100), 1);
assert.strictEqual(p.clampPage(2, 0, 100), 1);

assert.strictEqual(p.describePage(0, 0, 0), "0 rows in table");
assert.strictEqual(p.describePage(250, 100, 100), "Rows 101 to 200 of 250 total, fetched 100");
assert.strictEqual(p.describePage(250, 200, 50), "Rows 201 to 250 of 250 total, fetched 50");
assert.strictEqual(p.describePage(250, 300, 0), "No rows on this page, 250 rows in table");

assert.strictEqual(p.exportLabel(3, 3), "Download CSV (all 3 rows)");
assert.strictEqual(p.exportLabel(100, 250), "Download CSV (this page only: 100 of 250 rows)");

console.log("paging tests passed");
