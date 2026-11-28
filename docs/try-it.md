# Try parth_database without building Rust

The demo bundle contains parth_db.exe and start.cmd. The web console is built into the exe.

1. Unzip parth_db_demo.zip into a new folder.
2. Run start.cmd. The server listens on 127.0.0.1:8787 only.
3. The browser opens http://127.0.0.1:8787/. Press Connect, use address 127.0.0.1:8787 and token reviewer-demo-token-2026.
4. Paste a query from the list below and press Run.

Sample queries, run one at a time:
CREATE TABLE t (id INT PRIMARY KEY, g TEXT, v INT)
INSERT INTO t VALUES (1,'a',10),(2,'a',20),(3,'b',5)
SELECT g, COUNT(*), SUM(v) FROM t GROUP BY g HAVING SUM(v) > 6 ORDER BY g

These were run through start.cmd on a fresh data directory. CREATE shows 0 row(s) affected and the table t appears in the list, INSERT shows 3 row(s) affected, and the last query returns one row: a, 2, 30.

Every table needs a PRIMARY KEY, and a CREATE TABLE without one is rejected.

The token reviewer-demo-token-2026 is a demo-only value in start.cmd. The server listens on 127.0.0.1 only and remote binding is off. Change the token before any other use.

If port 8787 is already in use the server exits with an address-in-use error (os error 10048). Close the other program or edit PARTH_DB_ADDR in start.cmd and use that address in the console.