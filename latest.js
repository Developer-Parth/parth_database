function createLatest() {
  var current = 0;
  return {
    begin: function () {
      current += 1;
      return current;
    },
    invalidate: function () {
      current += 1;
    },
    isCurrent: function (id) {
      return id === current;
    }
  };
}

if (typeof module !== "undefined") {
  module.exports = { createLatest: createLatest };
}
