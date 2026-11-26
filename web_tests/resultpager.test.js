var assert = require("assert");
var paging = require("../paging.js");
var createResultPager = require("../resultpager.js").createResultPager;

function makeNode(tag) {
  var node = { tag: tag, children: [], className: "", handlers: {} };
  var text = "";
  Object.defineProperty(node, "textContent", {
    get: function () { return text; },
    set: function (value) { text = value; node.children = []; }
  });
  node.appendChild = function (child) { node.children.push(child); };
  return node;
}

function setup() {
  var log = { grids: [], downloads: [] };
  var env = {
    doc: { createElement: makeNode },
    pageSize: paging.PAGE_SIZE,
    pageCount: paging.pageCount,
    clampPage: paging.clampPage,
    pageOffset: paging.pageOffset,
    renderGrid: function (target, columns, rows) { log.grids.push(rows); },
    pagerButton: function (text, disabled, onClick) {
      var b = makeNode("button");
      b.label = text;
      b.disabled = disabled;
      b.click = onClick;
      return b;
    },
    downloadCsv: function (columns, rows) { log.downloads.push(rows.length); }
  };
  var target = makeNode("div");
  return { log: log, target: target, pager: createResultPager(env) };
}

function rowsOf(n) {
  var rows = [];
  for (var i = 1; i <= n; i++) { rows.push([i, "v" + i]); }
  return rows;
}

function bar(s) { return s.target.children[0].children; }
function button(s, label) { return bar(s).filter(function (n) { return n.label && n.label.indexOf(label) === 0; })[0]; }
function info(s) { return bar(s).filter(function (n) { return n.tag === "span"; })[0].textContent; }
function lastGrid(s) { return s.log.grids[s.log.grids.length - 1]; }

var s = setup();
s.pager(s.target, ["id", "v"], rowsOf(250), true);
assert.strictEqual(info(s), "Page 1 of 3. Showing 100 of 250 returned rows");
assert.strictEqual(button(s, "Previous").disabled, true);
assert.strictEqual(button(s, "Next").disabled, false);
assert.strictEqual(lastGrid(s)[0][0], 1);
assert.strictEqual(lastGrid(s).length, 100);
button(s, "Next").click();
assert.strictEqual(info(s), "Page 2 of 3. Showing 100 of 250 returned rows");
assert.strictEqual(lastGrid(s)[0][0], 101);
button(s, "Next").click();
assert.strictEqual(info(s), "Page 3 of 3. Showing 50 of 250 returned rows");
assert.strictEqual(lastGrid(s)[0][0], 201);
assert.strictEqual(lastGrid(s).length, 50);
assert.strictEqual(button(s, "Next").disabled, true);
button(s, "Previous").click();
assert.strictEqual(lastGrid(s)[0][0], 101);

var dl = button(s, "Download CSV");
assert.strictEqual(dl.label, "Download CSV (all 250 returned rows)");
dl.click();
assert.deepStrictEqual(s.log.downloads, [250]);

var exact = setup();
exact.pager(exact.target, ["id", "v"], rowsOf(100), true);
assert.strictEqual(info(exact), "Page 1 of 1. Showing 100 of 100 returned rows");
assert.strictEqual(button(exact, "Next").disabled, true);

var over = setup();
over.pager(over.target, ["id", "v"], rowsOf(101), true);
assert.strictEqual(info(over), "Page 1 of 2. Showing 100 of 101 returned rows");
button(over, "Next").click();
assert.strictEqual(lastGrid(over).length, 1);
assert.strictEqual(lastGrid(over)[0][0], 101);

var empty = setup();
empty.pager(empty.target, ["id", "v"], [], true);
assert.strictEqual(info(empty), "Page 1 of 1. Showing 0 of 0 returned rows");
assert.strictEqual(button(empty, "Download CSV").disabled, true);

var rolled = setup();
rolled.pager(rolled.target, ["id", "v"], rowsOf(250), false);
assert.strictEqual(button(rolled, "Download CSV"), undefined);
assert.strictEqual(button(rolled, "Next").disabled, false);
button(rolled, "Next").click();
assert.strictEqual(button(rolled, "Download CSV"), undefined);
assert.strictEqual(rolled.log.downloads.length, 0);

var rerun = setup();
rerun.pager(rerun.target, ["id", "v"], rowsOf(10), true);
rerun.pager(rerun.target, ["id", "v"], rowsOf(10), true);
assert.strictEqual(rerun.target.children.length, 2);

console.log("resultpager tests passed");
