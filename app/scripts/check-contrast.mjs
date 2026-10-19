// Checks every text and status colour pair in src/ui/tokens.ts against WCAG 2.2 AA (4.5:1), in the
// light and dark schemes (docs/frontend 14; roadmap 12). Exits 1 when a pair falls short.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const source = fs.readFileSync(path.resolve(here, "../src/ui/tokens.ts"), "utf8");

function scheme(name) {
  const block = source.match(new RegExp(`export const ${name}: ColorRoles = \\{([\\s\\S]*?)\\};`))?.[1];
  if (!block) throw new Error(`No ${name} scheme in tokens.ts`);
  return Object.fromEntries([...block.matchAll(/(\w+): "(#[0-9A-Fa-f]{6})"/g)].map((m) => [m[1], m[2]]));
}

function luminance(hex) {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function ratio(a, b) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

// Foreground on background, as the components draw them.
const pairs = [
  ["text", "background"],
  ["text", "surface"],
  ["text", "surfaceMuted"],
  ["text", "wash"],
  ["textMuted", "wash"],
  ["textMuted", "background"],
  ["textMuted", "surface"],
  ["textMuted", "surfaceMuted"],
  ["accentText", "accent"],
  ["accent", "surface"],
  ["accent", "background"],
  ["accent", "accentSoft"],
  ["danger", "surface"],
  ["danger", "dangerSoft"],
  ["info", "infoSoft"],
  ["info", "surface"],
  ...["healthy", "watch", "sick", "dead"].flatMap((k) => [
    [k, `${k}Soft`],
    [k, "surface"],
  ]),
];

let failed = false;
for (const name of ["light", "dark"]) {
  const c = scheme(name);
  const results = pairs.map(([fg, bg]) => ({ fg, bg, r: ratio(c[fg], c[bg]) }));
  const low = results.filter((x) => x.r < 4.5);
  const min = Math.min(...results.map((x) => x.r));
  console.log(`${name}: ${results.length} pairs, lowest ${min.toFixed(2)}:1${low.length ? "" : ", all pass AA"}`);
  for (const x of low) {
    failed = true;
    console.log(`  FAIL ${x.fg} on ${x.bg}: ${x.r.toFixed(2)}:1`);
  }
}
process.exit(failed ? 1 : 0);
