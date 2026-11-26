# parth_database architecture

## Storage
- One data file, `data.pdb`, made of 4096-byte pages, plus a `LOCK` file.
- Pages hold a copy-on-write B+tree. A write never edits a live page. It writes new pages, then switches the root by writing one of two meta pages.
- Every page carries a CRC. A torn or corrupt page is detected on read.
- The two meta pages alternate. On open, the valid meta page with the highest sequence number wins, so a crash during a write leaves the previous tree intact.
- Pages freed by a write are reused later.

## Tables and SQL
- Tables and their schemas are stored in the same B+tree as the rows.
- The SQL layer has a lexer, a parser and an executor. Supported: CREATE TABLE, INSERT, SELECT with WHERE, ORDER BY and LIMIT, UPDATE, DELETE, DROP TABLE.
- Column types: INT, REAL, TEXT, BOOL. A single primary key column is used as the B-tree key.

## Limits
- A row is at most 1000 bytes and a key at most 500 bytes. There are no overflow pages.
- Internal B-tree nodes are not merged after deletes.
- One writer at a time. No multi-statement transactions.
- No JOIN, GROUP BY, aggregates, secondary indexes or ALTER.

## HTTP API
- Endpoints: GET /api/health, GET /api/tables, GET /api/tables/{name}/rows, POST /api/sql.
- The server uses only the Rust standard library. Request bodies are limited to 1 MB. Statements run one at a time.

## Security boundary
- PARTH_DB_TOKEN is required. The server exits with code 2 if it is missing or shorter than 16 characters. The server never generates a token.
- All endpoints except /api/health need `Authorization: Bearer <token>`.
- The server binds to loopback only unless PARTH_DB_ALLOW_REMOTE is set.
- There is no TLS and no rate limiting. CORS allows any origin, so the token is the only protection. Do not expose the server to an untrusted network.
- The web page keeps the token in memory only.