# Running and using parth_database

## Build and test

    cd engine

    cargo build
    cargo tes
t

## S

tart the server



Set a secret token of at least 16 characters. The server refuses to start without it. Pick your own value and keep it out of the repository.

    $env:PARTH_DB_DIR 

= "D:\parth_data"
    $e
nv:PARTH_DB_TOKEN = "<your secret, 16+ characters>"
 
   $env:PARTH_DB_ADDR = "127.0.0.1:8787"
    cargo r
un

P

ARTH_DB_DIR is where the database files live. PARTH_DB_ADDR defaults to 127.0.0.1:8787. Non-loopback addresses are refused unless PARTH_DB_ALLOW_REMOTE=1 is set. There is no TLS, so do not expose the port to a network.


## Use the frontend

Serve


 the repository root with any static file server, for example `python -m http.server 8000 --bind 127.0.0.1`, and open http://127.0.0.1:8000/. Enter the server address and the token, then press Connect. Click a table to see its rows. Click a truncated cell, or focus it with Tab and press Enter or Space, to show its full value. If the server stops answering, the page keeps the last known data, dims it and marks it stale until a request succeeds again.



## Use the API directly


Every endpoint except /api/he
alth needs the header `Authorization: Bearer <token>`.

  

  Invoke-RestMethod http://127.0.0.1:8787/api/tables -Headers @{Authorization="Bearer <token>"}


## S
QL

    CR

EATE TABLE notes (id INT PRIMARY KEY, body TEXT)
  
  INSERT INTO notes VALUES (1, 'hello')

    SELECT * FROM notes WHERE id = 1 ORDER BY id LIMIT 10
    UP
DATE notes SET body = 'changed' WHERE id = 1
    
DELETE FROM notes WHERE id = 1
    D
ROP TABLE notes

Column 

types are INT, REAL, TEXT and BOOL. Every table needs exactly one PRIMARY KEY column. A row must fit in 1000 bytes. There are no transactions, JOINs or GROUP BY.

## Aggregates

SELECT accepts COUNT, SUM, MIN, MAX and AVG, alone or combined in one select list:

    SELECT COUNT(*), MIN(age), MAX(age) FROM users WHERE score > 1;

Rules that are tested:

- The result is always one row, with columns named like count(*) and min(age).
- NULL values are skipped. COUNT(*) counts every matching row. With no matching rows COUNT is 0 and the others are NULL.
- SUM of INT columns is INT and fails with an error on integer overflow. AVG is REAL. A REAL result that is not finite is an error.
- SUM and AVG need a numeric column. Only COUNT accepts *.
- LIMIT 0 returns no rows, any larger LIMIT returns the one row.
- ORDER BY is rejected with aggregates. Mixing plain columns with aggregates is rejected. GROUP BY is not supported yet.

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
- Limits: no savepoints, no sessions that span requests, and the whole transaction is held in memory until COMMIT.
