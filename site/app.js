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
  { file: "ok/tower.fcd", title: "Twisting tower: angle, ratio and count as kinds ✓" },
  { file: "ok/building.fcd", title: "Office building: 4 façades, floors, core ✓" },
  { file: "ok/facade.fcd", title: "Curtain-wall façade, 8 × 6 bays ✓" },
  { file: "err/fig5_swap.fcd", title: "Fig. 5 swap → E0102 kind mismatch" },
  { file: "ok/fig5_panel.fcd", title: "Fig. 5 panel, typed ✓" },
  { file: "ok/glazing.fcd", title: "User-defined unit and kind; Mass inferred ✓" },
  { file: "err/relabel.fcd", title: "Relabelling an Inset as a Thickness → E0102" },
  { file: "err/dimension_mismatch.fcd", title: "Length + Time → E0101" },
  { file: "err/unknown_unit.fcd", title: "Unknown unit → E0104" },
  { file: "err/param_missing_type.fcd", title: "Syntax: missing type → E0002" },
  { file: "err/unexpected_char.fcd", title: "Syntax: unexpected character → E0001" },
];

// A Curve or Surface input has no value in the language; the view draws it
// as a bay of this size.
const BASE = { w: 4, h: 3 };

const MATERIALS = {
  panel: { color: 0x3f9a63, opacity: 0.9 },
  glass: { color: 0x8fc1e3, opacity: 0.4 },
  metal: { color: 0x4a525c, opacity: 1 },
  fin: { color: 0xb08a5a, opacity: 1 },
  concrete: { color: 0xb9b4aa, opacity: 1 },
  core: { color: 0x9a9184, opacity: 1 },
  plate: { color: 0xe9e5dd, opacity: 1 },
  rib: { color: 0xf6f3ee, opacity: 1 },
  bad: { color: 0xd9483b, opacity: 0.55 },
};
const CURVE_COLOR = 0x8a8f98;
const SURFACE_COLOR = 0x6c8ebf;

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
let fitNext = true; // frame the camera on the next geometry (after loading an example)

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
  fitNext = true;
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
//
// Curves and surfaces are rectangles in the façade plane (x across, y up),
// with centre (cx, cy), at depth z (towards the viewer). A solid is a list
// of boxes; a box with negative depth d is inverted.

const isNumber = (x) => typeof x === "number";
const isCurve = (x) => x?.kind === "curve";
const isSurface = (x) => x?.kind === "surface";
const isSolid = (x) => x?.kind === "solid";

const box = (cx, cy, z, w, h, d, material) => ({ cx, cy, cz: z + d / 2, w, h, d, material });
const solid = (...parts) => ({ kind: "solid", parts });

// The bounding box of a solid's parts.
function extent(s) {
  const e = { minX: Infinity, maxX: -Infinity, minY: Infinity, maxY: -Infinity, minZ: Infinity, maxZ: -Infinity };
  for (const p of s.parts) {
    const half = Math.abs(p.d) / 2;
    e.minX = Math.min(e.minX, p.cx - p.w / 2);
    e.maxX = Math.max(e.maxX, p.cx + p.w / 2);
    e.minY = Math.min(e.minY, p.cy - p.h / 2);
    e.maxY = Math.max(e.maxY, p.cy + p.h / 2);
    e.minZ = Math.min(e.minZ, p.cz - half);
    e.maxZ = Math.max(e.maxZ, p.cz + half);
  }
  return e;
}

// Turn a part by quarter turns about the vertical axis: one turn takes the
// outward direction +z to +x. Width and depth swap at each turn.
function turn(p, turns) {
  let q = p;
  for (let i = 0; i < ((turns % 4) + 4) % 4; i++) {
    q = { ...q, cx: q.cz, cz: -q.cx, w: Math.abs(q.d), d: Math.sign(q.d || 1) * q.w };
  }
  return q;
}

const move = (p, dx, dz) => ({ ...p, cx: p.cx + dx, cz: p.cz + dz });

const BUILTINS = {
  // The prelude (paper Fig. 5)
  offset_curve: (c, d) => (isCurve(c) && isNumber(d) ? { ...c, input: false, w: c.w + 2 * d, h: c.h + 2 * d } : undefined),
  patch: (c) => (isCurve(c) ? { ...c, input: false, kind: "surface" } : undefined),
  offset_surface: (s, d) => (isSurface(s) && isNumber(d) ? { ...s, input: false, z: s.z + d } : undefined),
  thicken: (s, t) => (isSurface(s) && isNumber(t) ? solid(box(s.cx, s.cy, s.z, s.w, s.h, t, "panel")) : undefined),
  area: (s) => (isSurface(s) ? s.w * s.h : undefined),

  // Declared by examples/ok/facade.fcd
  rect: (w, h) => (isNumber(w) && isNumber(h) ? { kind: "curve", cx: 0, cy: h / 2, w, h, z: 0 } : undefined),
  inner: (c, d) => (isCurve(c) && isNumber(d) ? { ...c, input: false, w: c.w - 2 * d, h: c.h - 2 * d } : undefined),
  frame: (c, fw, fd) =>
    isCurve(c) && isNumber(fw) && isNumber(fd)
      ? solid(
          box(c.cx, c.cy + c.h / 2 - fw / 2, c.z, c.w, fw, fd, "metal"),
          box(c.cx, c.cy - c.h / 2 + fw / 2, c.z, c.w, fw, fd, "metal"),
          box(c.cx - c.w / 2 + fw / 2, c.cy, c.z, fw, c.h - 2 * fw, fd, "metal"),
          box(c.cx + c.w / 2 - fw / 2, c.cy, c.z, fw, c.h - 2 * fw, fd, "metal"),
        )
      : undefined,
  glaze: (s, t) => (isSurface(s) && isNumber(t) ? solid(box(s.cx, s.cy, s.z, s.w, s.h, t, "glass")) : undefined),
  slab: (c, t) => (isCurve(c) && isNumber(t) ? solid(box(c.cx, c.cy - c.h / 2 + t / 2, c.z - 1.2, c.w, t, 1.35, "concrete")) : undefined),
  fin: (c, d) => (isCurve(c) && isNumber(d) ? solid(box(c.cx - c.w / 2, c.cy, c.z, 0.05, c.h, d, "fin")) : undefined),
  union: (a, b) => (isSolid(a) && isSolid(b) ? solid(...a.parts, ...b.parts) : undefined),
  grid: (s, nx, ny, dx, dy) => {
    if (!isSolid(s) || ![nx, ny, dx, dy].every(isNumber)) return undefined;
    const cols = Math.max(0, Math.min(40, Math.round(nx)));
    const rows = Math.max(0, Math.min(40, Math.round(ny)));
    const parts = [];
    for (let i = 0; i < cols; i++) {
      for (let j = 0; j < rows; j++) {
        for (const p of s.parts) {
          parts.push({ ...p, cx: p.cx + (i - (cols - 1) / 2) * dx, cy: p.cy + j * dy });
        }
      }
    }
    return solid(...parts);
  },
  enclose: (front, side) => {
    if (!isSolid(front) || !isSolid(side) || !front.parts.length || !side.parts.length) return undefined;
    const f = extent(front);
    const s = extent(side);
    const halfW = (f.maxX - f.minX) / 2;
    const halfD = (s.maxX - s.minX) / 2;
    const shell = solid(
      ...front.parts.map((p) => move(p, 0, halfD)),
      ...front.parts.map((p) => move(turn(p, 2), 0, -halfD)),
      ...side.parts.map((p) => move(turn(p, 1), halfW, 0)),
      ...side.parts.map((p) => move(turn(p, 3), -halfW, 0)),
    );
    shell.footprint = { w: 2 * halfW, d: 2 * halfD };
    return shell;
  },
  storeys: (shell, n, h, t) => {
    if (!isSolid(shell) || ![n, h, t].every(isNumber)) return undefined;
    const e = extent(shell);
    const w = shell.footprint?.w ?? e.maxX - e.minX;
    const d = shell.footprint?.d ?? e.maxZ - e.minZ;
    const levels = Math.max(0, Math.min(60, Math.round(n)));
    const parts = [];
    for (let j = 0; j <= levels; j++) {
      parts.push({ cx: 0, cy: j * h, cz: 0, w, h: t, d, material: "concrete" }); // the last one is the roof
    }
    return solid(...parts);
  },
  core: (shell, size) => {
    if (!isSolid(shell) || !isNumber(size)) return undefined;
    const e = extent(shell);
    return solid({ cx: 0, cy: (e.minY + e.maxY) / 2, cz: 0, w: size, h: e.maxY - e.minY, d: size, material: "core" });
  },

  // Declared by examples/ok/tower.fcd. A swept form is a surface that
  // remembers how it was made; its layers are drawn by buildSwept below.
  oval: (a, b) => (isNumber(a) && isNumber(b) ? { kind: "curve", oval: { a, b } } : undefined),
  sweep: (c, n, h, twist, taper, sway) =>
    c?.oval && [n, h, twist, taper, sway].every(isNumber)
      ? { kind: "surface", form: { ...c.oval, n: Math.max(1, Math.min(120, Math.round(n))), h, twist, taper, sway } }
      : undefined,
  skin: (s) => (s?.form ? solid({ type: "skin", form: s.form, material: "glass" }) : undefined),
  plates: (s, t) => (s?.form && isNumber(t) ? solid({ type: "plates", form: s.form, thickness: t, material: "plate" }) : undefined),
  ribs: (s, n, d) => (s?.form && isNumber(n) && isNumber(d) ? solid({ type: "ribs", form: s.form, count: n, depth: d, material: "rib" }) : undefined),
  spine: (s, w) => (s?.form && isNumber(w) ? solid({ type: "spine", form: s.form, width: w, material: "core" }) : undefined),
};


function evaluate(result) {
  const env = new Map();
  const notes = [];
  const stepNames = new Set(result.steps.map((s) => s.name));
  const used = new Set(result.steps.flatMap((s) => s.args.filter((a) => "name" in a).map((a) => a.name)));

  // Inputs: numbers from the checker; geometry params get a default shape.
  const inputs = [];
  for (const b of result.bindings) {
    if (b.value !== null) {
      env.set(b.name, b.value);
    } else if ((b.type === "Curve" || b.type === "Surface") && !stepNames.has(b.name)) {
      const kind = b.type === "Curve" ? "curve" : "surface";
      env.set(b.name, { kind, cx: 0, cy: BASE.h / 2, w: BASE.w, h: BASE.h, z: 0, input: true });
      inputs.push(b.name);
    }
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
  }

  // Draw the inputs, and the final results: steps that no later step uses.
  const shapes = inputs.map((name) => ({ name, ...env.get(name) }));
  for (const step of result.steps) {
    const value = env.get(step.name);
    if (!used.has(step.name) && typeof value === "object") shapes.push({ name: step.name, ...value });
  }
  return { shapes, notes };
}

// Swept forms (tower example). A floor outline is a superellipse (a rounded
// rectangle) that, going up, turns by `twist`, scales by `taper` with a slight
// belly, and drifts sideways by `sway`. t is the height fraction, 0 to 1.

const SEGMENTS = 96; // points around one floor outline

function formPoint(f, u, t, offset = 0) {
  const ease = t * t * (3 - 2 * t); // smooth start and end of the twist
  const scale = (1 + (f.taper - 1) * t) * (1 + 0.1 * Math.sin(Math.PI * t));
  const angle = f.twist * ease;
  const e = 2 / 2.6;
  const cu = Math.cos(u);
  const su = Math.sin(u);
  const px = (f.a * scale + offset) * Math.sign(cu) * Math.abs(cu) ** e;
  const pz = (f.b * scale + offset) * Math.sign(su) * Math.abs(su) ** e;
  const sx = f.sway * Math.sin(Math.PI * t);
  return new THREE.Vector3(
    sx + px * Math.cos(angle) - pz * Math.sin(angle),
    t * f.n * f.h,
    px * Math.sin(angle) + pz * Math.cos(angle),
  );
}

function formCenter(f, t) {
  return new THREE.Vector3(f.sway * Math.sin(Math.PI * t), t * f.n * f.h, 0);
}

// The geometries of one swept part, in world coordinates.
function buildSwept(p) {
  const f = p.form;
  if (p.type === "skin") {
    const rows = f.n * 2;
    const row = SEGMENTS + 1;
    const positions = [];
    for (let r = 0; r <= rows; r++) {
      for (let i = 0; i <= SEGMENTS; i++) {
        const v = formPoint(f, (i / SEGMENTS) * 2 * Math.PI, r / rows);
        positions.push(v.x, v.y, v.z);
      }
    }
    const index = [];
    for (let r = 0; r < rows; r++) {
      for (let i = 0; i < SEGMENTS; i++) {
        const a = r * row + i;
        index.push(a, a + row, a + 1, a + 1, a + row, a + row + 1);
      }
    }
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geometry.setIndex(index);
    geometry.computeVertexNormals();
    return [geometry];
  }
  if (p.type === "plates") {
    const thickness = Math.max(0.02, Math.abs(p.thickness));
    const geometries = [];
    for (let k = 0; k <= f.n; k++) {
      const t = k / f.n;
      const shape = new THREE.Shape();
      for (let i = 0; i < SEGMENTS; i++) {
        // The bands stick out 45 cm beyond the glass.
        const v = formPoint(f, (i / SEGMENTS) * 2 * Math.PI, t, 0.45);
        // ExtrudeGeometry works in the xy plane; rotateX below lays it flat.
        if (i === 0) shape.moveTo(v.x, -v.z);
        else shape.lineTo(v.x, -v.z);
      }
      const geometry = new THREE.ExtrudeGeometry(shape, { depth: thickness, bevelEnabled: false });
      geometry.rotateX(-Math.PI / 2);
      geometry.translate(0, t * f.n * f.h - thickness, 0);
      geometries.push(geometry);
    }
    return geometries;
  }
  if (p.type === "ribs") {
    const count = Math.max(0, Math.min(200, Math.round(p.count)));
    const radius = Math.max(0.03, Math.abs(p.depth) / 2);
    const geometries = [];
    for (let i = 0; i < count; i++) {
      const u = (i / count) * 2 * Math.PI;
      const points = [];
      for (let k = 0; k <= f.n * 2; k++) points.push(formPoint(f, u, k / (f.n * 2), radius + 0.15));
      geometries.push(new THREE.TubeGeometry(new THREE.CatmullRomCurve3(points), f.n * 3, radius, 6, false));
    }
    return geometries;
  }
  if (p.type === "spine") {
    const points = [];
    for (let k = 0; k <= 16; k++) points.push(formCenter(f, k / 16));
    return [new THREE.TubeGeometry(new THREE.CatmullRomCurve3(points), 64, Math.max(0.5, p.width / 2), 24, true)];
  }
  return [];
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
  view.show(model, accepted, fitNext);
  if (model.shapes.length > 0) fitNext = false;

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
  const camera = new THREE.PerspectiveCamera(40, 1, 0.1, 500);
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

  // Put the camera so that all the geometry is in view.
  function fit() {
    const bounds = new THREE.Box3().setFromObject(content);
    if (bounds.isEmpty()) return;
    const size = bounds.getSize(new THREE.Vector3());
    const center = bounds.getCenter(new THREE.Vector3());
    const radius = Math.max(size.x, size.y, size.z, 2);
    const distance = (radius / (2 * Math.tan((camera.fov * Math.PI) / 360))) * 1.4;
    const direction = new THREE.Vector3(0.55, 0.3, 1).normalize();
    camera.position.copy(center).addScaledVector(direction, distance);
    camera.near = distance / 100;
    camera.far = distance * 20;
    camera.updateProjectionMatrix();
    controls.target.copy(center);
    grid.scale.setScalar(Math.max(1, radius / 8));
    controls.update();
  }
  labels.domElement.addEventListener("dblclick", fit);

  renderer.setAnimationLoop(() => {
    controls.update();
    renderer.render(scene, camera);
    labels.render(scene, camera);
  });

  function show(model, accepted, refit) {
    for (const child of [...content.children]) {
      child.traverse((o) => {
        o.geometry?.dispose();
        o.material?.dispose();
      });
      content.remove(child);
    }
    const materials = new Map(); // one material per style, shared by all boxes
    for (const shape of model.shapes) content.add(drawShape(shape, accepted, materials));
    if (refit) fit();
  }

  return { show };
}

function drawShape(shape, accepted, materials) {
  const group = new THREE.Group();

  if (shape.kind === "curve" || shape.kind === "surface") {
    if (!isNumber(shape.w)) return group; // ovals and swept forms are drawn by their layers
    if (shape.w <= 0 || shape.h <= 0) {
      group.add(label(`${shape.name}: collapsed (size ≤ 0)`, shape.cx, shape.cy, shape.z, "bad"));
      return group;
    }
    if (shape.kind === "curve") {
      const line = rectangle(shape.w, shape.h, shape.input);
      line.position.set(shape.cx, shape.cy, shape.z);
      group.add(line);
      group.add(label(shape.name, shape.cx - shape.w / 2, shape.cy + shape.h / 2 + 0.15, shape.z, "muted"));
    } else {
      const material = new THREE.MeshLambertMaterial({
        color: SURFACE_COLOR,
        transparent: true,
        opacity: 0.18,
        side: THREE.DoubleSide,
        depthWrite: false,
      });
      const mesh = new THREE.Mesh(new THREE.PlaneGeometry(shape.w, shape.h), material);
      mesh.position.set(shape.cx, shape.cy, shape.z);
      group.add(mesh);
    }
    return group;
  }

  // A solid: one box per part.
  const bounds = new THREE.Box3();
  const withEdges = shape.parts.length <= 20;
  let inverted = false;
  for (const p of shape.parts) {
    if (p.type) {
      // A swept part (tower example): built from its form, not a box.
      const style = accepted ? p.material : "bad";
      for (const geometry of buildSwept(p)) {
        group.add(new THREE.Mesh(geometry, material(materials, style)));
        geometry.computeBoundingBox();
        bounds.union(geometry.boundingBox);
      }
      continue;
    }
    if (p.w <= 0 || p.h <= 0 || Math.abs(p.d) < 1e-9) continue;
    inverted ||= p.d < 0;
    const style = !accepted || p.d < 0 ? "bad" : (p.material in MATERIALS ? p.material : "panel");
    const geometry = boxGeometry(materials, p.w, p.h, Math.abs(p.d));
    const mesh = new THREE.Mesh(geometry, material(materials, style));
    mesh.position.set(p.cx, p.cy, p.cz);
    group.add(mesh);
    if (withEdges) {
      const edges = new THREE.LineSegments(new THREE.EdgesGeometry(geometry), new THREE.LineBasicMaterial({ color: MATERIALS[style].color }));
      edges.position.copy(mesh.position);
      group.add(edges);
    }
    bounds.expandByPoint(new THREE.Vector3(p.cx - p.w / 2, p.cy - p.h / 2, p.cz - Math.abs(p.d) / 2));
    bounds.expandByPoint(new THREE.Vector3(p.cx + p.w / 2, p.cy + p.h / 2, p.cz + Math.abs(p.d) / 2));
  }
  if (bounds.isEmpty()) {
    group.add(label(`${shape.name}: empty solid`, 0, BASE.h / 2, 0, "bad"));
    return group;
  }

  const size = bounds.getSize(new THREE.Vector3());
  const center = bounds.getCenter(new THREE.Vector3());
  let text;
  if (shape.parts.length === 1 && !shape.parts[0].type) {
    const p = shape.parts[0];
    const warning = p.d < 0 ? " (negative: inverted solid)" : "";
    text = `${shape.name}: ${fmt(p.w)} × ${fmt(p.h)} m, thickness ${fmt(p.d)} m${warning}`;
  } else {
    text = `${shape.name}: ${shape.parts.length} parts, ${fmt(size.x)} × ${fmt(size.y)} m`;
  }
  const bad = !accepted || inverted;
  group.add(label(text, center.x, bounds.max.y + 0.35, center.z, bad ? "bad" : "ok"));
  return group;
}

function material(cache, style) {
  if (!cache.has(style)) {
    const { color, opacity } = MATERIALS[style];
    // Transparent parts do not write depth, so glass never hides the glass behind it.
    cache.set(
      style,
      new THREE.MeshLambertMaterial({ color, transparent: opacity < 1, opacity, depthWrite: opacity >= 1, side: THREE.DoubleSide }),
    );
  }
  return cache.get(style);
}

// One BoxGeometry per size, shared by all boxes of that size (same cache as the materials).
function boxGeometry(cache, w, h, d) {
  const key = `box:${w}|${h}|${d}`;
  if (!cache.has(key)) cache.set(key, new THREE.BoxGeometry(w, h, d));
  return cache.get(key);
}


function rectangle(w, h, dashed) {
  const corners = [[-w / 2, -h / 2], [w / 2, -h / 2], [w / 2, h / 2], [-w / 2, h / 2], [-w / 2, -h / 2]];
  const geometry = new THREE.BufferGeometry().setFromPoints(corners.map(([x, y]) => new THREE.Vector3(x, y, 0)));
  const lineMaterial = dashed
    ? new THREE.LineDashedMaterial({ color: CURVE_COLOR, dashSize: 0.15, gapSize: 0.1 })
    : new THREE.LineBasicMaterial({ color: CURVE_COLOR });
  const line = new THREE.Line(geometry, lineMaterial);
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
