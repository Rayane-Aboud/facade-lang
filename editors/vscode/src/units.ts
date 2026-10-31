export interface UnitDecl {
    name: string;
    meaning: string;
    source: string;
}

const UNIT_DECL =
  /^\s*unit\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?::\s*([A-Za-z_][A-Za-z0-9_]*)|=\s*(.*?))\s*(?:--.*)?$/gm;

export function parseUnits(text: string, source: string): UnitDecl[] {
  const units: UnitDecl[] = [];
  for (const m of text.matchAll(UNIT_DECL)) {
    const [, name, baseOf, scale] = m;
    const meaning = baseOf !== undefined ? `base unit of ${baseOf}` : scale;
    units.push({ name, meaning, source });
  }
  return units;
}


export function wantsUnit(before: string): boolean {
  if (before.includes("--")) return false;
  const afterNumber = /(?:^|[^A-Za-z0-9_.])\d+(?:\.\d+)?(?:[eE][+-]?\d+)?\s+[A-Za-z_]*$/;
  const insideBrackets = /\[[^\]]*$/;
  return afterNumber.test(before) || insideBrackets.test(before);
}

export function wantsPlusMinus(before: string): boolean {
  return before.endsWith("+/-");
}
