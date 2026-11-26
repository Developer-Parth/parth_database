var PAGE_SIZE = 100;

function pageCount(total, size) {
  return total === 0 ? 1 : Math.ceil(total / size);
}

function pageOffset(page, size) {
  return (page - 1) * size;
}

function clampPage(page, total, size) {
  return Math.min(Math.max(1, page), pageCount(total, size));
}

function describePage(total, offset, fetched) {
  if (total === 0) { return "0 rows in table"; }
  if (fetched === 0) { return "No rows on this page, " + total + " rows in table"; }
  return "Rows " + (offset + 1) + " to " + (offset + fetched) + " of " + total + " total, fetched " + fetched;
}

function exportLabel(fetched, total) {
  if (fetched === total) { return "Download CSV (all " + total + " rows)"; }
  return "Download CSV (this page only: " + fetched + " of " + total + " rows)";
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    PAGE_SIZE: PAGE_SIZE,
    pageCount: pageCount,
    pageOffset: pageOffset,
    clampPage: clampPage,
    describePage: describePage,
    exportLabel: exportLabel
  };
}
