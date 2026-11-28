const output = document.getElementById("output");
const input = document.getElementById("sql");
let wasm = null;

async function load() {
  const response = await fetch("parth_db.wasm");
  const result = await WebAssembly.instantiate(await response.arrayBuffer(), {});
  wasm = result.instance.exports;
}

function run(text) {
  const bytes = new TextEncoder().encode(text);
  const ptr = wasm.input_buffer(bytes.length);
  if (ptr === 0) {
    return { error: "input is too large" };
  }
  new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
  const length = wasm.run(bytes.length);
  const out = new Uint8Array(wasm.memory.buffer, wasm.output_buffer(), length);
  return JSON.parse(new TextDecoder().decode(out));
}

document.getElementById("run").addEventListener("click", () => {
  output.textContent = JSON.stringify(run(input.value), null, 2);
});

document.getElementById("reset").addEventListener("click", () => {
  wasm.reset();
  output.textContent = "database reset";
});

load().then(
  () => { output.textContent = "ready"; },
  (e) => { output.textContent = "failed to load wasm: " + e; }
);