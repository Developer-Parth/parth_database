Result limits

Output-row cap

Every query result is capped at 10000 rows (MAX_RESULT_ROWS in engine/src/exec.rs). The cap is an output-row cap only. It is not a memory bound.

The cap is applied after filtering, ordering, offset, grouping and aggregation, so the rows that are kept are the first 10000 rows of the exact result the SQL describes. SQL meaning is not changed. No early-stop shortcut is used on aggregate, group or order paths.

Explicit LIMIT versus safety truncation

An explicit LIMIT in the SQL is part of the query. A result that ends because of LIMIT is complete and is reported with truncated: false.

When the result had more rows than the cap and the cap removed some, the response carries truncated: true. This is safety truncation. The API response from POST /api/sql includes the truncated field.

How the UI shows it

The result pager shows "Truncated: the server capped this result, more rows matched than are shown" next to the page counter. The CSV button is relabelled "Download CSV (truncated: first N rows only, not the full result)". A truncated export is never presented as complete.

Table browsing pages through explicit LIMIT and OFFSET and reports the true row count in the total field.

Remaining unbounded paths

These are not bounded yet and the cap does not protect them:

All matching rows are materialised in memory before ordering, grouping, offset and the cap are applied. Aggregates without GROUP BY are the exception, described below.
Wide values (large TEXT values) are not bounded by the row cap. 10000 wide rows can still be a large response.
ORDER BY and GROUP BY read every matching row.
There is no byte limit on a response.

Bounded aggregate scan

Aggregates without GROUP BY visit rows in chunks of 64 under one store lock. The scan sees a single consistent state and a concurrent writer waits until it finishes. The scan stops with an error above 1000000 rows or 128 MiB of encoded row data.

The byte figure is an encoded-size budget, not a cap on heap use. Overflow values up to 1 MiB are read whole, so one chunk can briefly hold about 64 MiB.

Inside a transaction with pending writes the scan falls back to the merged read and is not bounded. GROUP BY, ORDER BY, UPDATE and DELETE are not covered and keep their original scan. The visitor must not call back into the store.

A plain SELECT without ORDER BY, GROUP BY or aggregates scans in chunks of 64. It skips OFFSET rows without keeping them and stops once it holds the 10000 row cap plus one extra row, which is how truncation is detected. A filter error on a row after that point is not raised because the row is never read. Inside a transaction with pending writes the store still builds the merged state in full before the scan, so that case is not bounded in memory, although the scan stops early.
ORDER BY with a LIMIT is the exception to the sentence above. The scan keeps a top-N buffer instead of sorting every row when keep is at most 20001, where keep is OFFSET plus the smaller of LIMIT and 10000, plus one, and a missing LIMIT counts as 10000. Otherwise the full sort is used. The scan still reads every matching row, so filter errors are raised as before. The buffer is trimmed back to keep rows once it reaches twice keep (at least 64), so it can hold about 40002 rows at most, and ties keep insertion order. One extra row is kept so truncation is still flagged. A small LIMIT can use the buffer with an OFFSET above 10000, up to the keep bound, while a query with no LIMIT or a LIMIT of 10000 or more needs an OFFSET of at most 10000. The effective cap is always 10000 rows whatever LIMIT was requested. Encoded row bytes and total heap are not bounded by this buffer, and the transaction path is not bounded. Plain aggregates, GROUP BY, UPDATE and DELETE materialize matching rows only up to 1,000,000 rows and 128 MiB, and fail with an error above that. This is a materialization bound on those paths, not a cap on total heap.  Regression tests cover the row bound and the byte bound for UPDATE and DELETE. GROUP BY has row and byte bound tests through select with an injected limit. ORDER BY beyond top-N has a byte bound test, also inside a transaction with a pending write. There the merged read is still built in full before the bound applies.

Request bodies are limited to 1 MB and request headers to 16 KB.

Not verified

Behaviour under memory pressure has not been tested. Power-loss durability has not been verified.