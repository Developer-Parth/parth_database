function runGuarded(latest, promise, onOk, onFail) {
  var seq = latest.begin();
  return promise.then(function (value) {
    if (latest.isCurrent(seq)) { return onOk(value); }
  }).catch(function (error) {
    if (latest.isCurrent(seq)) { onFail(error); }
  });
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = { runGuarded: runGuarded };
}
