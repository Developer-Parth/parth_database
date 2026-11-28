# parth_database


A small relational database engine written in Rust, with an HTTP API and a browser console. It is a learning project, not a production database.

## Try it without building Rust

Unzip the demo bundle, run start.cmd, and the console opens at http://127.0.0.1:8787/. The demo token and sample queries are in docs/try-it.md.

## Browser playground (local only)

The engine also builds to WebAssembly and runs an in-memory database in a browser page. It is a separate path from the disk engine: nothing is saved and no server is involved. It is not hosted anywhere.

To rebuild the module, run `rustup target add wasm32-unknown-unknown`, then from engine/ run `cargo build --release --target wasm32-unknown-unknown` and copy target/wasm32-unknown-unknown/release/parth_db.wasm to playground/parth_db.wasm.

To use it, serve the playground folder from a local static server, for example `python -m http.server 8002 --bind 127.0.0.1 --directory playground`, and open http://127.0.0.1:8002/index.html. Opening index.html straight from disk does not load the module.

`node web_tests/wasm_playground.test.js` checks the module, including 2000 reset-and-run cycles with an unchanged WebAssembly memory size. That is not a general leak-free claim.

## Build from source

From the engine folder run cargo build --release, and cargo test for the engine tests. The web tests run with node, one file at a time, from web_tests.

## What is tested

At the last full run all engine tests and all web tests passed. Crash tests use a hard kill of the process followed by a reopen. Aggregate and rollback tests reopen the process.

## Not verified or not supported

Power-loss durability is not verified. JOIN and UNION are rejected, and so is unsupported SQL in general. LIKE is case-sensitive and has no ESCAPE. The 1,000,000 row and 128 MiB bound limits materialized results and is not a heap cap. Every table needs a PRIMARY KEY. The demo token in start.cmd is for local demo use only.

## Docs

docs/try-it.md, docs/usage.md, docs/result-limits.md and docs/architecture.md. The Windows demo ZIP is on the release page https://github.com/Developer-Parth/parth_database/releases/tag/v0.1.0-demo