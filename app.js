(function () {
  var state = { base: "", token: "", table: "", page: 1 };
  var rowsLatest = createLatest();
var sqlLatest = createLatest();
  var el = function (id) { return document.getElementById(id); };

  function setStatus(text, kind) {
    var s = el("status");
    s.textContent = text;
    s.className = "status " + kind;
  }

  function request(method, path, body) {
    var options = { method: method, headers: { Authorization: "Bearer " + state.token } };
    if (body !== undefined) {
      options.headers["Content-Type"] = "application/json";
      options.body = JSON.stringify(body);
    }
    return fetch(state.base + path, options).then(function (response) {
      return response.text().then(function (text) {
        var data = null;
        try { data = JSON.parse(text); } catch (e) { data = null; }
        if (!response.ok) {
          var message = data && data.error ? data.error : "HTTP " + response.status;
          var err = new Error(message);
          err.status = response.status;
          throw err;
        }
        document.body.classList.remove("stale");
        setStatus("Connected to " + state.base, "on");
      switchHistory();
        return data;
      });
    });
  }

  function cell(value) {
    var td = document.createElement("td");
    if (value === null || value === undefined) {
      td.textContent = "NULL";
      td.className = "null";
    } else {
      td.textContent = String(value);
      td.title = String(value);
    }
    td.tabIndex = 0;
    td.addEventListener("click", function () { td.classList.toggle("open"); });
    td.addEventListener("keydown", function (event) {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        td.classList.toggle("open");
      }
    });
    return td;
  }

  function renderGrid(target, columns, rows) {
    target.textContent = "";
    var table = document.createElement("table");
    var head = document.createElement("tr");
    columns.forEach(function (name) {
      var th = document.createElement("th");
      th.textContent = name;
      head.appendChild(th);
    });
    table.appendChild(head);
    rows.forEach(function (row) {
      var tr = document.createElement("tr");
      row.forEach(function (value) { tr.appendChild(cell(value)); });
      table.appendChild(tr);
    });
    target.appendChild(table);
  }

  function describeColumns(columns) {
    return columns.map(function (c) {
      return c.name + " " + c.type + (c.primary_key ? " (primary key)" : "");
    }).join(", ");
  }

  function pagerButton(text, disabled, onClick) {
    var b = document.createElement("button");
    b.type = "button";
    b.textContent = text;
    b.disabled = disabled;
    b.addEventListener("click", onClick);
    return b;
  }

  function renderPager(name, columns, data) {
    var pager = el("pager");
    pager.textContent = "";
    var last = pageCount(data.total, PAGE_SIZE);
    var prev = pagerButton("Previous", state.page <= 1, function () { loadRows(name, columns, state.page - 1); });
    var next = pagerButton("Next", state.page >= last, function () { loadRows(name, columns, state.page + 1); });
    var info = document.createElement("span");
    info.textContent = "Page " + state.page + " of " + last + ". " + describePage(data.total, data.offset, data.rows.length);
    var csv = pagerButton(exportLabel(data.rows.length, data.total), data.rows.length === 0, function () { downloadCsv(data.columns, data.rows); });
    [prev, next, info, csv].forEach(function (node) { pager.appendChild(node); });
  }

  function loadRows(name, columns, page) {
    state.table = name;
    state.page = page || 1;
    var seq = rowsLatest.begin();
    el("view-title").textContent = "Rows: " + name;
    el("schema").textContent = describeColumns(columns);
    var query = "?limit=" + PAGE_SIZE + "&offset=" + pageOffset(state.page, PAGE_SIZE);
    request("GET", "/api/tables/" + encodeURIComponent(name) + "/rows" + query).then(function (data) {
      if (!rowsLatest.isCurrent(seq)) {
        return;
      }
      if (state.page > pageCount(data.total, PAGE_SIZE)) {
        loadRows(name, columns, clampPage(state.page, data.total, PAGE_SIZE));
        return;
      }
      renderGrid(el("rows-wrap"), data.columns, data.rows);
      renderPager(name, columns, data);
      var buttons = el("tables").querySelectorAll("button");
      Array.prototype.forEach.call(buttons, function (b) {
        b.className = b.dataset.name === name ? "active" : "";
      });
    }).catch(function (error) {
      if (rowsLatest.isCurrent(seq)) {
        showError(error);
      }
    });
  }

  function loadTables() {
    return request("GET", "/api/tables").then(function (data) {
      var list = el("tables");
      list.textContent = "";
      el("tables-empty").textContent = data.tables.length ? "" : "No tables yet.";
      data.tables.forEach(function (t) {
        var li = document.createElement("li");
        var button = document.createElement("button");
        button.type = "button";
        button.dataset.name = t.name;
        button.textContent = t.name;
        button.addEventListener("click", function () { loadRows(t.name, t.columns); });
        li.appendChild(button);
        list.appendChild(li);
      });
      if (state.table) {
        var again = data.tables.filter(function (t) { return t.name === state.table; })[0];
        if (again) { loadRows(again.name, again.columns, state.page); }
      }
    });
  }

  function showError(err) {
    if (err instanceof TypeError || err.status === 401) { document.body.classList.add("stale"); }
    if (err instanceof TypeError) {
      setStatus("Cannot reach server (blocked, offline or mixed content)", "err");
    } else if (err.status === 401) {
      setStatus("Token rejected", "err");
    } else {
      el("message").textContent = err.message;
      el("message").className = "error";
    }
  }

    function describe(data) {
    return data.columns.length ? data.rows.length + " row(s)" : data.affected + " row(s) affected";
  }

  function addExport(target, columns, rows) {
  var button = document.createElement("button");
  button.type = "button";
  button.textContent = "Download CSV";
  button.addEventListener("click", function () { downloadCsv(columns, rows); });
  target.insertBefore(button, target.firstChild);
}

  var pagedResult = createResultPager({ doc: document, pageSize: PAGE_SIZE, pageCount: pageCount, clampPage: clampPage, pageOffset: pageOffset, renderGrid: renderGrid, pagerButton: pagerButton, downloadCsv: downloadCsv });
function showResult(data) {
    var wrap = el("result-wrap");
    var message = el("message");
    message.className = "muted";
    if (!Array.isArray(data.results)) {
      message.textContent = describe(data);
      pagedResult(wrap, data.columns, data.rows, true, data.truncated === true);
      if (!data.columns.length) { wrap.textContent = ""; }
      return;
    }
    message.textContent = (data.committed ? "Committed: " : "Rolled back: ") + data.results.length + " statement(s)";
    message.className = data.committed ? "muted" : "error";
    wrap.textContent = "";
    data.results.forEach(function (result, index) {
      var line = document.createElement("p");
      line.textContent = "Statement " + (index + 1) + ": " + describe(result) + (data.committed ? "" : " (rolled back, not saved)");
      wrap.appendChild(line);
      if (result.columns.length) {
        var holder = document.createElement("div");
        holder.className = "grid-wrap";
        pagedResult(holder, result.columns, result.rows, data.committed !== false, result.truncated === true);
        wrap.appendChild(holder);
      }
    });
  }

var queryHistory = [];

function getStorage() {
  try {
    return window.localStorage;
  } catch (err) {
    return null;
  }
}

function renderHistory() {
  var select = el("history");
  select.textContent = "";
  var first = document.createElement("option");
  first.value = "";
  first.textContent = queryHistory.length ? "History (" + queryHistory.length + ")" : "History empty";
  select.appendChild(first);
  queryHistory.forEach(function (sql, index) {
    var option = document.createElement("option");
    var flat = sql.replace(/\s+/g, " ");
    option.value = String(index);
    option.textContent = flat.length > 60 ? flat.slice(0, 57) + "..." : flat;
    select.appendChild(option);
  });
}

function switchHistory() {
  queryHistory = isRemembering(getStorage()) ? loadHistory(getStorage(), state.base) : [];
  renderHistory();
}

function rememberQuery(sql) {
  queryHistory = pushHistory(queryHistory, sql);
  if (isRemembering(getStorage())) { saveHistory(getStorage(), state.base, queryHistory); }
  renderHistory();
}

function runSql() {
    var sql = el("sql").value.trim();
    if (!sql || !state.token) { return; }
    rememberQuery(sql);
    var message = el("message");
    message.className = "muted";
    message.textContent = "Running";
    runGuarded(sqlLatest, request("POST", "/api/sql", { sql: sql }), function (data) {
      showResult(data);
      return loadTables();
    }, function (err) {
      el("result-wrap").textContent = "";
      showError(err);
    });
  }

  el("connect").addEventListener("submit", function (event) {
    event.preventDefault();
    rowsLatest.invalidate();
    sqlLatest.invalidate();
    state.base = el("server").value.replace(/\/+$/, "");
    state.token = el("token").value;
    state.table = "";
    el("result-wrap").textContent = "";
    el("message").textContent = "";
    setStatus("Connecting", "off");
    fetch(state.base + "/api/health").then(function (response) {
      if (!response.ok) { throw new Error("health check failed"); }
      return loadTables();
    }).then(function () {
      setStatus("Connected to " + state.base, "on");
      switchHistory();
    }).catch(function (err) {
      state.token = "";
      showError(err);
      if (!(err instanceof TypeError) && err.status !== 401) { setStatus(err.message, "err"); }
    });
  });

  el("run").addEventListener("click", runSql);
  el("history").addEventListener("change", function () {
    var index = el("history").value;
    if (index !== "") {
      el("sql").value = queryHistory[Number(index)];
      el("sql").focus();
    }
    el("history").value = "";
  });
  el("clear-history").addEventListener("click", function () {
    queryHistory = [];
    saveHistory(getStorage(), state.base, queryHistory);
    renderHistory();
  });
  queryHistory = [];
  el("remember").checked = isRemembering(getStorage());
  el("remember").addEventListener("change", function () {
    var storage = getStorage();
    setRemembering(storage, el("remember").checked);
    if (el("remember").checked) {
      saveHistory(storage, state.base, queryHistory);
    } else {
      forgetAll(storage);
    }
  });
  renderHistory();
  el("sql").addEventListener("keydown", function (event) {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      runSql();
    }
  });
})();