var HISTORY_PREFIX = "parth_db_history:";
var REMEMBER_KEY = "parth_db_remember_history";
var HISTORY_LIMIT = 25;

function pushHistory(list, sql) {
  var trimmed = String(sql).trim();
  if (!trimmed) { return list.slice(); }
  var rest = list.filter(function (item) { return item !== trimmed; });
  rest.unshift(trimmed);
  return rest.slice(0, HISTORY_LIMIT);
}

function loadHistory(storage, base) {
  if (!storage || !base) { return []; }
  try {
    var parsed = JSON.parse(storage.getItem(HISTORY_PREFIX + base) || "[]");
    if (!Array.isArray(parsed)) { return []; }
    return parsed.filter(function (item) { return typeof item === "string" && item.length > 0; }).slice(0, HISTORY_LIMIT);
  } catch (err) {
    return [];
  }
}

function saveHistory(storage, base, list) {
  if (!storage || !base) { return false; }
  try {
    if (list.length === 0) {
      storage.removeItem(HISTORY_PREFIX + base);
    } else {
      storage.setItem(HISTORY_PREFIX + base, JSON.stringify(list));
    }
  } catch (err) {
    return false;
  }
  return true;
}

function isRemembering(storage) {
  if (!storage) { return false; }
  try {
    return storage.getItem(REMEMBER_KEY) === "1";
  } catch (err) {
    return false;
  }
}

function setRemembering(storage, on) {
  if (!storage) { return false; }
  try {
    if (on) {
      storage.setItem(REMEMBER_KEY, "1");
    } else {
      storage.removeItem(REMEMBER_KEY);
    }
  } catch (err) {
    return false;
  }
  return true;
}

function forgetAll(storage) {
  if (!storage) { return false; }
  try {
    var doomed = [];
    for (var i = 0; i < storage.length; i++) {
      var name = storage.key(i);
      if (name && name.indexOf(HISTORY_PREFIX) === 0) { doomed.push(name); }
    }
    doomed.forEach(function (name) { storage.removeItem(name); });
  } catch (err) {
    return false;
  }
  return true;
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    pushHistory: pushHistory,
    loadHistory: loadHistory,
    saveHistory: saveHistory,
    isRemembering: isRemembering,
    setRemembering: setRemembering,
    forgetAll: forgetAll,
    HISTORY_LIMIT: HISTORY_LIMIT
  };
}
