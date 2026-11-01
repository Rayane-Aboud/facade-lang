// FacadeLang playground. The checker is the Rust implementation compiled to
// WebAssembly (site/pkg, built by build.sh). This file connects it to the
// editor, the diagnostics and types panels, and the 3D view.

import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { CSS2DObject, CSS2DRenderer } from "three/addons/renderers/CSS2DRenderer.js";
import init, { check, prelude } from "./pkg/facadelang.js";

// The example programs, copied from examples/ by build.sh. The first one is
// shown when the page opens.
const EXAMPLES = [
  { file: "err/fig5_swap.fcd", title: "Fig. 5 swap → E0102 kind mismatch" },
  { file: "ok/fig5_panel.fcd", title: "Fig. 5 panel, typed ✓" },
  { file: "ok/glazing.fcd", title: "User-defined unit and kind; Mass inferred ✓" },
  { file: "err/relabel.fcd", title: "Relabelling an Inset as a Thickness → E0102" },
  { file: "err/dimension_mismatch.fcd", title: "Length + Time → E0101" },
  { file: "err/unknown_unit.fcd", title: "Unknown unit → E0104" },
  { file: "err/param_missing_type.fcd", title: "Syntax: missing type → E0002" },
  { file: "err/unexpected_char.fcd", title: "Syntax: unexpected character → E0001" },
];

// A Curve input has no value in the language; the view draws it as a bay of this size.
const BASE = { w: 4, h: 3 };

const COLORS = { ok: 0x3f9a63, bad: 0xd9483b, curve: 0x8a8f98, surface: 0x6c8ebf };

// Editor

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
});
const select = document.getElementById("example");
let marks = [];
let timer = null;
let view = null;

async function main() {
  for (const example of EXAMPLES) {
    const option = document.createElement("option");
    option.value = example.file;
    option.textContent = example.title;
    select.append(option);
  }
  setupTabs();
  try {
    view = createView(document.getElementById("view"));
  } catch (err) {
    setBanner("bad", `The 3D view needs WebGL, which is not available here (${err.message}).`);
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
  render(result);
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
    out.className = "tab-panel diagnostics ok";
    setStatus("ok", `✓ accepted · checked in ${ms.toFixed(1)} ms`);
  } else {
    out.textContent = diagnostics.map((d) => d.rendered).join("\n");
    out.className = "tab-panel diagnostics bad";
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

function setupTabs() {
  const tabs = document.querySelectorAll(".tab");
  for (const tab of tabs) {
    tab.addEventListener("click", () => {
      for (const other of tabs) {
        other.classList.toggle("active", other === tab);
        document.getElementById(other.dataset.tab).hidden = other !== tab;
      }
    });
  }
}

// Geometry: a tiny untyped interpreter, like a visual programming tool. It
// runs every step of the dataflow, whatever the checker says. The built-ins
// are opaque in the language; these stand-ins exist only to draw something.

const isNumber = (x) => typeof x === "number";
const isCurve = (x) => x?.kind === "curve";
const isSurface = (x) => x?.kind === "surface";

// Shapes are rectangles in the façade plane (x across, y up), centred on the
// bay, at depth z (towards the viewer). Solids extend from z by `depth`.
const BUILTINS = {
  offset_curve: (c, d) => (isCurve(c) && isNumber(d) ? { kind: "curve", w: c.w + 2 * d, h: c.h + 2 * d, z: c.z } : undefined),
  patch: (c) => (isCurve(c) ? { kind: "surface", w: c.w, h: c.h, z: c.z } : undefined),
  offset_surface: (s, d) => (isSurface(s) && isNumber(d) ? { kind: "surface", w: s.w, h: s.h, z: s.z + d } : undefined),
  thicken: (s, t) => (isSurface(s) && isNumber(t) ? { kind: "solid", w: s.w, h: s.h, z: s.z, depth: t } : undefined),
  area: (s) => (isSurface(s) ? s.w * s.h : undefined),
};

function evaluate(result) {
  const env = new Map();
  const shapes = [];
  const notes = [];
  // Inputs: numbers from the checker; geometry inputs get a default shape.
  for (const b of result.bindings) {
    if (b.value !== null) env.set(b.name, b.value);
    else if (b.type === "Curve") env.set(b.name, { kind: "curve", w: BASE.w, h: BASE.h, z: 0, input: true });
    else if (b.type === "Surface") env.set(b.name, { kind: "surface", w: BASE.w, h: BASE.h, z: 0, input: true });
  }
  for (const step of result.steps) {
    const args = step.args.map((a) => ("name" in a ? env.get(a.name) : a.value));
    const builtin = BUILTINS[step.call];
    const out = builtin ? builtin(...args) : undefined;
    if (out === undefined) {
      notes.push(`${step.name} = ${step.call}(…) is not drawn`);
      continue;
    }
    env.set(step.name, out);
    if (typeof out === "object") shapes.push({ name: step.name, ...out });
  }
  for (const [name, value] of env) {
    if (typeof value === "object" && value.input) shapes.unshift({ name, ...value });
  }
  return { shapes, notes };
}

function render(result) {
  if (!view) return;
  // While a line is half-typed, keep the last geometry instead of flickering.
  if (result.diagnostics.some((d) => d.code.startsWith("E000"))) {
    setBanner("", "Syntax error: showing the last geometry that could be built.");
    return;
  }
  const accepted = result.diagnostics.length === 0;
  const model = evaluate(result);
  view.show(model, accepted);

  const notes = model.notes.length ? ` (${model.notes.join("; ")})` : "";
  if (model.shapes.length === 0) {
    setBanner(accepted ? "ok" : "bad", `${accepted ? "✓ Accepted" : "✗ Rejected"}. This program builds no geometry.${notes}`);
  } else if (accepted) {
    setBanner("ok", `✓ Accepted. The geometry is built from the program.${notes}`);
  } else {
    const codes = [...new Set(result.diagnostics.map((d) => d.code))].join(", ");
    setBanner("bad", `✗ Rejected by FacadeLang (${codes}). In red: what an untyped tool such as Dynamo would still build.${notes}`);
  }
}

// 3D view (three.js)

function createView(container) {
  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
  renderer.setPixelRatio(window.devicePixelRatio);
  container.append(renderer.domElement);

  const labels = new CSS2DRenderer();
  labels.domElement.className = "labels";
  container.append(labels.domElement);

  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(40, 1, 0.1, 200);
  camera.position.set(6.5, 4.5, 9);
  // The labels layer lies on top of the canvas, so it receives the mouse.
  const controls = new OrbitControls(camera, labels.domElement);
  controls.target.set(0, BASE.h / 2, 0);
  controls.enableDamping = true;

  scene.add(new THREE.HemisphereLight(0xffffff, 0x808080, 2.0));
  const sun = new THREE.DirectionalLight(0xffffff, 1.5);
  sun.position.set(5, 8, 6);
  scene.add(sun);
  const grid = new THREE.GridHelper(12, 24, 0x888888, 0x888888);
  grid.material.transparent = true;
  grid.material.opacity = 0.35;
  scene.add(grid);

  const content = new THREE.Group();
  scene.add(content);

  function resize() {
    const w = container.clientWidth;
    const h = Math.max(container.clientHeight, 1);
    renderer.setSize(w, h);
    labels.setSize(w, h);
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
  }
  new ResizeObserver(resize).observe(container);
  resize();

  renderer.setAnimationLoop(() => {
    controls.update();
    renderer.render(scene, camera);
    labels.render(scene, camera);
  });

  function show(model, accepted) {
    for (const child of [...content.children]) {
      child.traverse((o) => {
        o.geometry?.dispose();
        o.material?.dispose();
      });
      content.remove(child);
    }
    for (const shape of model.shapes) content.add(drawShape(shape, accepted));
  }

  return { show };
}

function drawShape(shape, accepted) {
  const group = new THREE.Group();
  const cy = BASE.h / 2;
  if (shape.w <= 0 || shape.h <= 0) {
    group.add(label(`${shape.name}: collapsed (size ≤ 0)`, 0, cy, shape.z, "bad"));
    return group;
  }

  if (shape.kind === "curve") {
    const line = rectangle(shape.w, shape.h, shape.input);
    line.position.set(0, cy, shape.z);
    group.add(line);
    group.add(label(shape.name, -shape.w / 2, cy + shape.h / 2 + 0.15, shape.z, "muted"));
  } else if (shape.kind === "surface") {
    const material = new THREE.MeshLambertMaterial({
      color: COLORS.surface,
      transparent: true,
      opacity: 0.18,
      side: THREE.DoubleSide,
      depthWrite: false,
    });
    const mesh = new THREE.Mesh(new THREE.PlaneGeometry(shape.w, shape.h), material);
    mesh.position.set(0, cy, shape.z);
    group.add(mesh);
  } else if (shape.kind === "solid") {
    const t = shape.depth;
    const bad = !accepted || t <= 0;
    const color = bad ? COLORS.bad : COLORS.ok;
    if (Math.abs(t) > 1e-9) {
      const box = new THREE.BoxGeometry(shape.w, shape.h, Math.abs(t));
      const material = new THREE.MeshLambertMaterial({ color, transparent: true, opacity: t < 0 ? 0.45 : 0.85 });
      const mesh = new THREE.Mesh(box, material);
      mesh.position.set(0, cy, shape.z + t / 2);
      const edges = new THREE.LineSegments(new THREE.EdgesGeometry(box), new THREE.LineBasicMaterial({ color }));
      edges.position.copy(mesh.position);
      group.add(mesh, edges);
    }
    const warning = t < 0 ? " (negative: inverted solid)" : "";
    const text = `${shape.name}: ${fmt(shape.w)} × ${fmt(shape.h)} m, thickness ${fmt(t)} m${warning}`;
    group.add(label(text, 0, cy + shape.h / 2 + 0.35, shape.z + t / 2, bad ? "bad" : "ok"));
  }
  return group;
}

function rectangle(w, h, dashed) {
  const corners = [[-w / 2, -h / 2], [w / 2, -h / 2], [w / 2, h / 2], [-w / 2, h / 2], [-w / 2, -h / 2]];
  const geometry = new THREE.BufferGeometry().setFromPoints(corners.map(([x, y]) => new THREE.Vector3(x, y, 0)));
  const material = dashed
    ? new THREE.LineDashedMaterial({ color: COLORS.curve, dashSize: 0.15, gapSize: 0.1 })
    : new THREE.LineBasicMaterial({ color: COLORS.curve });
  const line = new THREE.Line(geometry, material);
  if (dashed) line.computeLineDistances();
  return line;
}

function label(text, x, y, z, tone) {
  const el = document.createElement("div");
  el.className = `label ${tone}`;
  el.textContent = text;
  const object = new CSS2DObject(el);
  object.position.set(x, y, z);
  return object;
}

// Small helpers

function fmt(v) {
  return String(Number(v.toPrecision(6)));
}

function setStatus(kind, message) {
  const el = document.getElementById("status");
  el.textContent = message;
  el.className = `status ${kind}`;
}

function setBanner(kind, message) {
  const el = document.getElementById("view-banner");
  el.textContent = message;
  el.className = `view-banner ${kind}`;
}

main();
