function createResultPager(env) {
  var doc = env.doc;
  return function pagedResult(target, columns, rows, allowExport, truncated) {
    var page = 1;
    var bar = doc.createElement("div");
    var grid = doc.createElement("div");
    bar.className = "result-pager";
    target.textContent = "";
    target.appendChild(bar);
    target.appendChild(grid);
    function draw() {
      var last = env.pageCount(rows.length, env.pageSize);
      page = env.clampPage(page, rows.length, env.pageSize);
      var offset = env.pageOffset(page, env.pageSize);
      var slice = rows.slice(offset, offset + env.pageSize);
      env.renderGrid(grid, columns, slice);
      var info = doc.createElement("span");
      info.textContent = "Page " + page + " of " + last + ". Showing " + slice.length + " of " + rows.length + " returned rows";
      var prev = env.pagerButton("Previous", page <= 1, function () { page = page - 1; draw(); });
      var next = env.pagerButton("Next", page >= last, function () { page = page + 1; draw(); });
      var nodes = [prev, next, info];
if (truncated) { info.textContent = info.textContent + ". Truncated: the server capped this result, more rows matched than are shown"; }
      if (allowExport) {
        nodes.push(env.pagerButton("Download CSV (all " + rows.length + " returned rows)", rows.length === 0, function () { env.downloadCsv(columns, rows); }));
      }
if (truncated && allowExport) { nodes[3].textContent = "Download CSV (truncated: first " + rows.length + " rows only, not the full result)"; }
      bar.textContent = "";
      nodes.forEach(function (node) { bar.appendChild(node); });
    }
    draw();
  };
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = { createResultPager: createResultPager };
}
