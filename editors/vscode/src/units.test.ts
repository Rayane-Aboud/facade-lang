import { strict as assert } from "node:assert";
import { test } from "node:test";
import { parseUnits, wantsPlusMinus, wantsUnit } from "./units";

test("parseUnits reads base and derived units", () => {
  const text = "unit m : Length\nunit mm = 0.001 m   -- millimetre\n";
  assert.deepEqual(parseUnits(text, "std.fcd"), [
    { name: "m", meaning: "base unit of Length", source: "std.fcd" },
    { name: "mm", meaning: "0.001 m", source: "std.fcd" },
  ]);
});

test("parseUnits ignores other declarations and comments", () => {
  const text = "kind Inset : Length\nlet unit_cost = 3 m\n-- unit cm = 0.01 m\n";
  assert.deepEqual(parseUnits(text, "x.fcd"), []);
});

test("wantsUnit after a number and a space", () => {
  assert.equal(wantsUnit("let a = 0.9 "), true);
  assert.equal(wantsUnit("let a = 0.9 m"), true);
});

test("wantsUnit inside brackets", () => {
  assert.equal(wantsUnit("let g = 9.81 [m/"), true);
});

test("wantsUnit is false elsewhere", () => {
  assert.equal(wantsUnit("let a = "), false);
  assert.equal(wantsUnit("let b = f(x2 "), false);
  assert.equal(wantsUnit("-- costs 3 "), false);
  assert.equal(wantsUnit("let g = 9.81 [m/s^2] "), false);
});

test("wantsPlusMinus only right after +/-", () => {
  assert.equal(wantsPlusMinus("let f = 10 mm +/-"), true);
  assert.equal(wantsPlusMinus("let f = 10 mm +"), false);
});
