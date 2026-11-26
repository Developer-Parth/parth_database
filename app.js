(function () {
  var state = { base: "", token: "", table: "" };
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
    }
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

  function describe(columns) {
    return columns.map(function (c) {
      return c.name + " " + c.type + (c.primary_key ? " (primary key)" : "");
    }).join(", ");
  }

  function loadRows(name, columns) {
    state.table = name;
    el("view-title").textContent = "Rows: " + name;
    el("schema").textContent = describe(columns);
    request("GET", "/api/tables/" + encodeURIComponent(name) + "/rows").then(function (data) {
      renderGrid(el("rows-wrap"), data.columns, data.rows);
      var buttons = el("tables").querySelectorAll("button");
      Array.prototype.forEach.call(buttons, function (b) {
        b.className = b.dataset.name === name ? "active" : "";
      });
    }).catch(showError);
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
        if (again) { loadRows(again.name, again.columns); }
      }
    });
  }

  function showError(err) {
    if (err instanceof TypeError) {
      setStatus("Cannot reach server (blocked, offline or mixed content)", "err");
    } else if (err.status === 401) {
      setStatus("Token rejected", "err");
    } else {
      el("message").textContent = err.message;
      el("message").className = "error";
    }
  }

  function runSql() {
    var sql = el("sql").value.trim();
    if (!sql || !state.token) { return; }
    var message = el("message");
    message.className = "muted";
    message.textContent = "Running";
    request("POST", "/api/sql", { sql: sql }).then(function (data) {
      message.textContent = data.columns.length ? data.rows.length + " row(s)" : data.affected + " row(s) affected";
      renderGrid(el("result-wrap"), data.columns, data.rows);
      if (!data.columns.length) { el("result-wrap").textContent = ""; }
      return loadTables();
    }).catch(function (err) {
      el("result-wrap").textContent = "";
      showError(err);
    });
  }

  el("connect").addEventListener("submit", function (event) {
    event.preventDefault();
    state.base = el("server").value.replace(/\/+$/, "");
    state.token = el("token").value;
    state.table = "";
    setStatus("Connecting", "off");
    fetch(state.base + "/api/health").then(function (response) {
      if (!response.ok) { throw new Error("health check failed"); }
      return loadTables();
    }).then(function () {
      setStatus("Connected to " + state.base, "on");
    }).catch(function (err) {
      state.token = "";
      showError(err);
      if (!(err instanceof TypeError) && err.status !== 401) { setStatus(err.message, "err"); }
    });
  });

  el("run").addEventListener("click", runSql);
  el("sql").addEventListener("keydown", function (event) {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      runSql();
    }
  });
})();