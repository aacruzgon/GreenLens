// WCAG 2.2 contrast for every text and status colour on background, surface and wash, in both schemes,
// plus the pairs the components draw (the pill's label, status chips, banners). Fails under 4.5:1
// (design.md 6; roadmap F0 item 7). Reads src/ui/palette.json, the values tokens.ts uses.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const palette = JSON.parse(readFileSync(join(here, "../src/ui/palette.json"), "utf8"));

const MIN = 4.5;
const TEXT = ["text", "textMuted", "accent", "danger", "info", "healthy", "watch", "sick", "dead"];
const GROUNDS = ["background", "surface", "wash"];
const EXTRA = [
  ["accentText", "accent"],
  ["accent", "accentText"],
  ["healthy", "healthySoft"],
  ["watch", "watchSoft"],
  ["sick", "sickSoft"],
  ["dead", "deadSoft"],
  ["info", "infoSoft"],
  ["danger", "dangerSoft"],
  ["text", "infoSoft"],
  ["text", "watchSoft"],
  ["text", "dangerSoft"],
  ["text", "surfaceMuted"],
  ["textMuted", "surfaceMuted"],
];

function luminance(hex) {
  const n = hex.replace("#", "");
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(n.slice(i, i + 2), 16) / 255);
  const lin = (c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

function ratio(a, b) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

let failed = 0;
for (const scheme of ["light", "dark"]) {
  const c = palette[scheme];
  const pairs = [...TEXT.flatMap((fg) => GROUNDS.map((bg) => [fg, bg])), ...EXTRA];
  const rows = pairs.map(([fg, bg]) => {
    if (!c[fg] || !c[bg]) throw new Error(`${scheme}: unknown role ${c[fg] ? bg : fg}`);
    return { fg, bg, r: ratio(c[fg], c[bg]) };
  });
  rows.sort((a, b) => a.r - b.r);
  const bad = rows.filter((x) => x.r < MIN);
  failed += bad.length;
  const low = rows[0];
  console.log(`${scheme}: ${rows.length} pairs, lowest ${low.r.toFixed(2)}:1 (${low.fg} on ${low.bg})`);
  for (const x of rows.slice(0, 3)) console.log(`  ${x.r.toFixed(2)}:1  ${x.fg} on ${x.bg}`);
  for (const x of bad) console.error(`  FAIL ${x.r.toFixed(2)}:1  ${x.fg} on ${x.bg} (${c[x.fg]} on ${c[x.bg]})`);
}

if (failed > 0) {
  console.error(`${failed} pair(s) under ${MIN}:1`);
  process.exit(1);
}
console.log(`All pairs at ${MIN}:1 or above.`);
