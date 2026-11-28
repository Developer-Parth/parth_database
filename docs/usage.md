# Running and using parth_database

## Build and test

    cd engine
    cargo build
    cargo test

## Start the server

Set a secret token of at least 16 characters. The server refuses to start without it. Pick your own value and keep it out of the repository.

    $env:PARTH_DB_DIR = "D:\parth_data"
    $env:PARTH_DB_TOKEN = "<your secret, 16+ characters>"
    $env:PARTH_DB_ADDR = "127.0.0.1:8787"
    cargo run

PARTH_DB_DIR is where the database files live. PARTH_DB_ADDR defaults to 127.0.0.1:8787. Non-loopback addresses are refused unless PARTH_DB_ALLOW_REMOTE=1 is set. There is no TLS, so do not expose the port to a network.

## Use the frontend

Serve the repository root with any static file server, for example `python -m http.server 8000 --bind 127.0.0.1`, and open http://127.0.0.1:8000/. Enter the server address and the token, then press Connect. Click a table to see its rows. Click a truncated cell, or focus it with Tab and press Enter or Space, to show its full value. If the server stops answering, the page keeps the last known data, dims it and marks it stale until a request succeeds again.

## Use the API directly

Every endpoint except /api/health needs the header `Authorization: Bearer <token>`.

    Invoke-RestMethod http://127.0.0.1:8787/api/tables -Headers @{Authorization="Bearer <token>"}

## SQL

    CREATE TABLE notes (id INT PRIMARY KEY, body TEXT)
    INSERT INTO notes VALUES (1, 'hello')
    SELECT * FROM notes WHERE id = 1 ORDER BY id LIMIT 10
    UPDATE notes SET body = 'changed' WHERE id = 1
    DELETE FROM notes WHERE id = 1
    DROP TABLE notes

Column types are INT, REAL, TEXT and BOOL. Every table needs exactly one PRIMARY KEY column. A key may be at most 500 bytes and a stored row up to 1 MiB (1048576 bytes). That storage limit is separate from the 1 MB HTTP request body limit. There are no JOINs. WHERE supports BETWEEN, NOT BETWEEN, IN and NOT IN. An IN list may hold at most 256 values. NULL follows SQL unknown semantics, so id NOT IN (1, NULL) matches no rows.

SELECT DISTINCT takes a plain column list, for example SELECT DISTINCT n FROM t ORDER BY n. It is not supported with *, aggregates or GROUP BY, and ORDER BY takes one column and it must appear in the DISTINCT list. NULLs collapse into one group, DISTINCT runs before LIMIT and OFFSET, and the usual materialization bounds apply. The byte budget is divided across the source rows and the projected copy, so repeated columns use more of it. The budget is an estimate of materialized data, not a verified heap cap.

## Aggregates

SELECT accepts COUNT, SUM, MIN, MAX and AVG, alone or combined in one select list:

    SELECT COUNT(*), MIN(age), MAX(age) FROM users WHERE score > 1;

Rules that are tested:

- The result is always one row, with columns named like count(*) and min(age).
- NULL values are skipped. COUNT(*) counts every matching row. With no matching rows COUNT is 0 and the others are NULL.
- SUM of INT columns is INT and fails with an error on integer overflow. AVG is REAL. A REAL result that is not finite is an error.
- SUM and AVG need a numeric column. Only COUNT accepts *.
- COUNT(DISTINCT col) counts the distinct non-NULL values of one column and names its result column count_distinct(col). DISTINCT is rejected with SUM, MIN, MAX and AVG, and COUNT(DISTINCT *) is rejected.
- Each COUNT(DISTINCT) keeps two temporary lists of references to the non-NULL values, the one evaluate builds and a sorted clone, so about 16 bytes per non-NULL row in total (the clone alone is 8). They are built one aggregate at a time and per group, stay inside the 1,000,000 row aggregate bound (about 16 MB at most) and are not counted in the byte budget. Several COUNT(DISTINCT) columns and GROUP BY are tested.
- LIMIT 0 returns no rows, any larger LIMIT returns the one row.
- A plain SELECT takes one or more ORDER BY columns, each with its own ASC or DESC, such as ORDER BY a DESC, b. NULLs sort first ascending and last descending on every key. ORDER BY without GROUP BY is rejected with aggregates. With GROUP BY, ORDER BY takes one column and may name a grouping column or an aggregate that is in the select list, written as in the select list (COUNT(*), SUM(qty)). NULL aggregate results sort first ascending and last descending. An aggregate missing from the select list is rejected. Mixing plain columns with aggregates is rejected. GROUP BY is described below.

## Transactions

One POST /api/sql request may carry a transaction:

    BEGIN; INSERT INTO users VALUES (4, 'dev'); UPDATE users SET name = 'bee' WHERE id = 2; COMMIT;

The response is {"committed": true, "results": [...]} with one result per statement. A request with a single plain statement behaves as before.

Rules that are tested:

- All statements run on a private overlay. Later statements see earlier uncommitted writes, deletes, table creation and table drops.
- COMMIT writes everything in one atomic B+tree commit. ROLLBACK runs the statements, returns their results with "committed": false and writes nothing.
- A failing statement fails the whole request and nothing is written. A transaction without COMMIT or ROLLBACK is an error and nothing is written.
- Rejected forms: nested BEGIN, COMMIT or ROLLBACK outside a transaction, statements after COMMIT, and several statements without BEGIN.
- Pending state lives only inside one request. Separate requests never share it.
- The server runs one request at a time, so a transaction has no concurrent writers.
- Limits: no savepoints, no sessions that span requests, and the whole transaction is held in memory until COMMIT. A transaction is rejected and rolled back when it writes more than 100,000 entries or 32 MiB of cumulative key and value bytes. This bounds pending writes and is not a heap cap.

## Transactions in the web page

A script that starts with BEGIN and ends with COMMIT or ROLLBACK can be pasted into the SQL box, one statement per line or all on one line. The page shows "Committed" or "Rolled back" with the statement count, then the result of each statement in order. After a ROLLBACK every statement line is labelled "rolled back, not saved", because those rows existed only inside the discarded transaction. A failing statement rejects the whole script and nothing is saved.
## GROUP BY

`SELECT region, COUNT(*), SUM(qty) FROM sales WHERE qty > 0 GROUP BY region ORDER BY region DESC LIMIT 5`

- Every plain column in the select list must appear in GROUP BY. Several grouping columns are allowed.
- COUNT, SUM, MIN, MAX and AVG are computed per group. WHERE filters rows before grouping.
- Groups come back in ascending order of the grouping key. ORDER BY takes one column, a grouping column or an aggregate, and it must be in the select list. Ties between equal aggregate values come back in no fixed order. LIMIT applies to groups.
- Rejected with an error: SELECT * with GROUP BY, a column next to an aggregate without GROUP BY, a column missing from GROUP BY, and HAVING without GROUP BY. HAVING takes conditions on aggregates whether or not they are in the select list (hidden ones are never returned), joined with AND or OR (AND binds tighter), such as HAVING COUNT(*) > 1 AND SUM(qty) > 5. It filters groups before ORDER BY and LIMIT, and a NULL aggregate fails it. Non-aggregate columns in HAVING are rejected.

## Web console

The web console is a browser client for the parth_database HTTP API. It does not store the API token. Connect with the server address and the token the API process was started with.

## Table view

- Rows are fetched with LIMIT and OFFSET on the server. The page size is 100 and the API caps a single request at 1000 rows.
- Rows have no guaranteed order beyond the scan order of the table. If rows are inserted or deleted between page requests, the pages can shift.
- Download CSV in the table view exports only the page currently shown.
- Responses that arrive late or out of order, including late errors, are ignored. Connecting to another server also drops any request still in flight.

## SQL box

- Run with the button or Ctrl+Enter.
- String literals use single quotes only. Unsupported SQL is rejected with an error.
- Results are paged in the browser, 100 rows per page. The API returns every row of the result in one response, so the browser holds the whole result in memory and there is no bound on network use or memory.
- Download CSV in the SQL box exports all returned rows, not only the visible page. The label shows the count.
- When a multi-statement request is rolled back, the result is shown without a CSV export.
- Late or out of order SQL responses and errors are ignored, and switching servers drops a request in flight.
- When the server cannot be reached the page shows Cannot 
reach server (blocked, offline or mixed content). A wrong token shows Token rejected and marks the last known data as stale. A statement the engine cannot parse shows its error, such as expected a statement. The Running label clears after any failed query.
## Query history

- History is kept in the browser only when Remember queries is ticked. It stores SQL text only, up to 25 entries per server, and never the token.
- Clear history removes the saved entries for the connected server.

## Tests

The browser modules are covered by plain node scripts in web_tests: csv, guarded, history, latest, paging and resultpager. Run each with node, for example node web_tests/paging.test.js. The resultpager test uses a fake DOM, not a real browser.

## Limits

Crash safety was tested by killing the process. Behaviour under power loss has not been verified. Aggregate and rollback tests reopen the process, they do not kill it. COUNT(DISTINCT) and text MIN and MAX inside HAVING are covered by tests. Ties in ORDER BY on an aggregate have no defined order.
