const assert = require("assert");
const fs = require("fs");
const path = require("path");

const bytes = fs.readFileSync(path.join(__dirname, "..", "playground", "parth_db.wasm"));
const wasm = new WebAssembly.Instance(new WebAssembly.Module(bytes), {}).exports;

function run(text) {
  const encoded = new TextEncoder().encode(text);
  const ptr = wasm.input_buffer(encoded.length);
  assert.notStrictEqual(ptr, 0);
  new Uint8Array(wasm.memory.buffer, ptr, encoded.length).set(encoded);
  const length = wasm.run(encoded.length);
  const out = new Uint8Array(wasm.memory.buffer, wasm.output_buffer(), length);
  return JSON.parse(new TextDecoder().decode(out));
}

const script = "BEGIN; CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT); INSERT INTO t VALUES (1, 'a'); SELECT * FROM t; COMMIT;";

wasm.reset();
const first = run(script);
assert.strictEqual(first.committed, true);
assert.deepStrictEqual(first.results[2].rows, [[1, "a"]]);

const again = run(script);
assert.ok(again.error.includes("table exists"));

wasm.reset();
const warm = run(script);
assert.strictEqual(warm.committed, true);
const before = wasm.memory.buffer.byteLength;
for (let i = 0; i < 2000; i++) {
  wasm.reset();
  const result = run(script);
  assert.strictEqual(result.committed, true);
}
assert.strictEqual(wasm.memory.buffer.byteLength, before);

wasm.reset();
run("CREATE TABLE u (id INTEGER PRIMARY KEY, name TEXT)");
run("INSERT INTO u VALUES (1, 'héllo ✓')");
assert.deepStrictEqual(run("SELECT * FROM u").rows, [[1, "héllo ✓"]]);

const badPtr = wasm.input_buffer(2);
new Uint8Array(wasm.memory.buffer, badPtr, 2).set([0xff, 0xfe]);
const badLength = wasm.run(2);
const badOut = new Uint8Array(wasm.memory.buffer, wasm.output_buffer(), badLength);
assert.ok(JSON.parse(new TextDecoder().decode(badOut)).error);

assert.strictEqual(wasm.input_buffer((1 << 20) + 1), 0);

wasm.input_buffer(4);
const overLength = wasm.run(100);
const overOut = new Uint8Array(wasm.memory.buffer, wasm.output_buffer(), overLength);
assert.ok(JSON.parse(new TextDecoder().decode(overOut)).error);

assert.ok(run("SELECT * FROM u JOIN u ON 1 = 1").error);

wasm.reset();
assert.ok(run("SELECT * FROM u").error);