// FacadeLang playground. The checker is the Rust implementation compiled to
// WebAssembly (site/pkg, built by build.sh). This file only connects it to
// the editor, the results panel and the drawing.

import init, { check, prelude } from "./pkg/facadelang.js";

// The example programs, copied from examples/ by build.sh. The first one is
// shown when the page opens.
const EXAMPLES = [
  { file: "err/fig5_swap.fcd", title: "Fig. 5 swap → E0102 kind mismatch" },
  { file: "ok/fig5_panel.fcd", title: "Fig. 5 panel, typed ✓" },
  { file: "err/relabel.fcd", title: "Relabelling an Inset as a Thickness → E0102" },
  { file: "err/dimension_mismatch.fcd", title: "Length + Time → E0101" },
  { file: "ok/glazing.fcd", title: "User-defined unit and kind; Mass inferred ✓" },
  { file: "err/unknown_unit.fcd", title: "Unknown unit → E0104" },
  { file: "err/param_missing_type.fcd", title: "Syntax: missing type → E0002" },
  { file: "err/unexpected_char.fcd", title: "Syntax: unexpected character → E0001" },
];

// Syntax highlighting: one regular expression per kind of token.
CodeMirror.defineSimpleMode("facadelang", {
  start: [
    { regex: /--.*/, token: "comment" },
    { regex: /\b(?:dimension|unit|kind|extern|param|let|where|and|component|plan|install|on)\b/, token: "keyword" },
    { regex: /\b(?:Curve|Surface|Solid|Tol)\b/, token: "builtin" },
    { regex: /[A-Z][A-Za-z0-9_]*/, token: "type" },
    { regex: /\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/, token: "number" },
    { regex: /±|\+\/-|->|[-+*\/^<>=]/, token: "operator" },
  ],
  meta: { lineComment: "--" },
});

const editor = CodeMirror.fromTextArea(document.getElementById("source"), {
  mode: "facadelang",
  lineNumbers: true,
  indentUnit: 2,
  viewportMargin: Infinity,
});
const select = document.getElementById("example");
let marks = [];
let timer = null;

async function main() {
  for (const example of EXAMPLES) {
    const option = document.createElement("option");
    option.value = example.file;
    option.textContent = example.title;
    select.append(option);
  }
  try {
    await init();
  } catch (err) {
    setStatus("bad", `could not load the checker (WebAssembly): ${err}`);
    return;
  }
  document.getElementById("prelude").textContent = prelude();
  editor.on("change", () => {
    clearTimeout(timer);
    timer = setTimeout(run, 150);
  });
  select.addEventListener("change", () => loadExample(select.value));
  await loadExample(EXAMPLES[0].file);
}

async function loadExample(file) {
  const response = await fetch(`examples/${file}`);
  if (!response.ok) {
    setStatus("bad", `could not load examples/${file}`);
    return;
  }
  editor.setValue(await response.text());
  run();
}

// Check the editor's text and update everything on the page.
function run() {
  const source = editor.getValue();
  const start = performance.now();
  const result = JSON.parse(check(source));
  const ms = performance.now() - start;
  showDiagnostics(result.diagnostics, ms);
  showBindings(result.bindings);
  drawPanel(source, result);
}

function showDiagnostics(diagnostics, ms) {
  for (const mark of marks) mark.clear();
  marks = diagnostics.map((d) => {
    const from = { line: d.line - 1, ch: d.column - 1 };
    // An empty span (for example at the end of the file) still gets one character.
    const endCh = d.end_line === d.line ? Math.max(d.end_column, d.column + 1) : d.end_column;
    const to = { line: d.end_line - 1, ch: endCh - 1 };
    return editor.markText(from, to, { className: "cm-error-mark", title: `${d.code}: ${d.message}` });
  });

  const out = document.getElementById("diagnostics");
  if (diagnostics.length === 0) {
    out.textContent = "✓ accepted: the program is well-typed.";
    out.className = "diagnostics ok";
    setStatus("ok", `✓ accepted · checked in ${ms.toFixed(1)} ms`);
  } else {
    out.textContent = diagnostics.map((d) => d.rendered).join("\n");
    out.className = "diagnostics bad";
    const n = diagnostics.length;
    setStatus("bad", `✗ ${n} error${n > 1 ? "s" : ""} · checked in ${ms.toFixed(1)} ms`);
  }
}

function showBindings(bindings) {
  const rows = bindings.map((b) => {
    const row = document.createElement("tr");
    for (const text of [b.name, b.type, b.value === null ? "—" : fmt(b.value)]) {
      const cell = document.createElement("td");
      cell.textContent = text;
      row.append(cell);
    }
    return row;
  });
  document.querySelector("#bindings tbody").replaceChildren(...rows);
}

// The drawing. It uses the SI values computed by the checker. The base curve
// is a Curve, which has no value, so it is drawn as a 4 m × 3 m bay.
function drawPanel(source, result) {
  const el = document.getElementById("drawing");
  const values = Object.fromEntries(result.bindings.map((b) => [b.name, b.value]));
  const isNumber = (name) => typeof values[name] === "number";

  if (!["inset", "offset", "thickness"].every(isNumber)) {
    el.innerHTML = `<p class="hint">The drawing shows the Fig. 5 panel: it appears when the program
      declares <code>inset</code>, <code>offset</code> and <code>thickness</code> with values.</p>`;
    return;
  }

  if (result.diagnostics.length === 0) {
    el.innerHTML = card(
      "FacadeLang: accepted. The panel as designed",
      figure(values.inset, values.offset, values.thickness, "ok"),
      "ok",
      "Left: front view (dashed: the base curve; filled: the panel after the inset). Right: section (offset from the base plane, then thickness).",
    );
    return;
  }

  // Which names were really passed to the geometry functions? Read from the
  // source text with a pattern: a shortcut for this demo, not part of the checker.
  const thickArg = argument(source, "thicken", 1);
  const insetArg = argument(source, "offset_curve", 1);
  const kindError = result.diagnostics.find((d) => d.code === "E0102");
  const rejected = card(
    "FacadeLang: rejected before anything is built",
    `<pre>${esc(result.diagnostics.map((d) => `error[${d.code}]: ${d.message}`).join("\n"))}</pre>`,
    "bad",
  );

  if (kindError && isNumber(thickArg) && isNumber(insetArg)) {
    const t = values[thickArg];
    const consequence = t <= 0 ? "a negative thickness: the solid is inverted." : "a panel with the wrong thickness.";
    const dynamo = card(
      "Dynamo: no error, and it builds this",
      figure(values[insetArg], values.offset, t, "bad"),
      "bad",
      `It uses <code>${esc(insetArg)}</code> as the inset and <code>${esc(thickArg)}</code> (${fmt(t)} m) as the thickness: ${consequence}`,
    );
    el.innerHTML = dynamo + rejected;
  } else {
    el.innerHTML = rejected;
  }
}

// One panel in two views, in metres: front view (left) and section (right).
// SVG's y axis points down, so a height h is drawn at y = H - h.
function figure(inset, offset, thickness, tone) {
  const W = 4;
  const H = 3;
  // offset_curve: a negative inset moves the outline inwards.
  const x0 = -inset;
  const x1 = W + inset;
  const top = H + inset;
  const bottom = -inset;
  const hasPanel = x1 > x0 && top > bottom;

  const left = Math.min(0, x0) - 0.4;
  const sx = Math.max(W, x1) + 1.6; // where the section view starts
  const a = sx + offset;
  const b = sx + offset + thickness;
  const right = Math.max(sx + 3.4, a, b) + 0.4;
  const yTop = Math.min(0, H - top) - 0.4;
  const labelY = Math.max(H, H - bottom) + 0.45;

  const parts = [`<rect class="base" x="0" y="0" width="${W}" height="${H}" stroke-dasharray="0.12 0.08"/>`];
  if (hasPanel) {
    parts.push(`<rect class="slab ${tone}" x="${x0}" y="${H - top}" width="${x1 - x0}" height="${top - bottom}"/>`);
  }
  parts.push(text(W / 2, labelY, `front · inset ${fmt(inset)} m`, "middle"));

  parts.push(`<line class="wall" x1="${sx}" y1="0" x2="${sx}" y2="${H}"/>`);
  if (thickness !== 0) {
    const slabTone = thickness < 0 ? "bad" : tone;
    const y = hasPanel ? H - top : 0;
    const h = hasPanel ? top - bottom : H;
    parts.push(`<rect class="slab ${slabTone}" x="${Math.min(a, b)}" y="${y}" width="${Math.abs(thickness)}" height="${h}"/>`);
  }
  parts.push(text(sx, labelY, `section · offset ${fmt(offset)} m`, "start"));
  parts.push(text(sx, labelY + 0.35, `thickness ${fmt(thickness)} m`, "start"));

  const viewBox = [left, yTop, right - left, labelY + 0.6 - yTop].join(" ");
  return `<svg viewBox="${viewBox}" stroke-width="0.03" font-size="0.24" role="img"
    aria-label="Panel: front view and section">${parts.join("")}</svg>`;
}

function card(title, body, tone, note = "") {
  const noteHtml = note ? `<p class="note">${note}</p>` : "";
  return `<figure class="card ${tone}"><figcaption>${title}</figcaption>${body}${noteHtml}</figure>`;
}

// The name passed as argument `index` of the first call to `fn`, or null.
function argument(source, fn, index) {
  const code = source.replace(/--.*$/gm, "");
  const match = code.match(new RegExp(`\\b${fn}\\s*\\(([^)]*)\\)`));
  const arg = match ? match[1].split(",")[index] : undefined;
  return arg ? arg.trim() : null;
}

function text(x, y, content, anchor) {
  return `<text x="${x}" y="${y}" text-anchor="${anchor}">${esc(content)}</text>`;
}

function fmt(v) {
  return String(Number(v.toPrecision(6)));
}

function esc(s) {
  return String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
}

function setStatus(kind, message) {
  const el = document.getElementById("status");
  el.textContent = message;
  el.className = `status ${kind}`;
}

main();
