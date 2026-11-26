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

All matching rows are materialised in memory before ordering, grouping, offset and the cap are applied.
Wide values (large TEXT values) are not bounded by the row cap. 10000 wide rows can still be a large response.
ORDER BY, GROUP BY and aggregates read every matching row.
There is no byte limit on a response.

Request bodies are limited to 1 MB and request headers to 16 KB.

Not verified

Behaviour under memory pressure has not been tested. Power-loss durability has not been verified.