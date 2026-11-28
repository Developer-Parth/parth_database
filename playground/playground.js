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
  if (length > 1 << 24) {
    return { error: "output is too large" };
  }
  const out = new Uint8Array(wasm.memory.buffer, wasm.output_buffer(), length);
  return JSON.parse(new TextDecoder().decode(out));
}

document.getElementById("run").addEventListener("click", () => { 
 output.textContent = format(run(input.value));
});

function format(result) {
  if (!result.results) {
    return JSON.stringify(result, null, 2);
  }
  const lines = ["committed: " + result.committed];
  for (const item of result.results) {
    lines.push(JSON.stringify(item));
  }
  return lines.join("\n");
}

document.getElementById("reset").addEventListener("click", () => {
  wasm.reset();
  output.textContent = "database reset";
});

load().then(
  () => { output.textContent = "ready"; },
  (e) => { output.textContent = "failed to load wasm: " + e; }
);