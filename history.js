var HISTORY_KEY = "parth_db_history";
var HISTORY_LIMIT = 25;

function pushHistory(list, sql) {
  var trimmed = String(sql).trim();
  if (!trimmed) { return list.slice(); }
  var rest = list.filter(function (item) { return item !== trimmed; });
  rest.unshift(trimmed);
  return rest.slice(0, HISTORY_LIMIT);
}

function loadHistory(storage) {
  try {
    var parsed = JSON.parse(storage.getItem(HISTORY_KEY) || "[]");
    if (!Array.isArray(parsed)) { return []; }
    return parsed.filter(function (item) { return typeof item === "string" && item.length > 0; }).slice(0, HISTORY_LIMIT);
  } catch (err) {
    return [];
  }
}

function saveHistory(storage, list) {
  try {
    storage.setItem(HISTORY_KEY, JSON.stringify(list));
  } catch (err) {
    return false;
  }
  return true;
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = { pushHistory: pushHistory, loadHistory: loadHistory, saveHistory: saveHistory, HISTORY_LIMIT: HISTORY_LIMIT };
}
