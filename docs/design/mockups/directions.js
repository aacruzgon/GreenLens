// The design directions shown in the mockups. Each one is data only: app.js turns it into
// custom properties and data attributes on the phone, and garden3d.js reads its garden3d block.
// Apple owns the chrome in every direction (glass tab bar and bar buttons, SF Pro in bars);
// a direction changes only the content layer (philosophies.md section 6).
//
// colors      the role names of app/src/ui/tokens.ts, light and dark (philosophies.md 7.4)
// fonts       display (content heroes only), body and number stacks; `google` lists families to load
// sections    "title" or "label" (small capitals); sectionFont "display" or "body"
// shape       radii in px; cardStyle border, rule, block or shadow; check circle or square
// layout      home large or hero; plants grid, rows or carousel; scan bar, edge or hero;
//             region box, ink, lasso or reticle; unsure plates or table; weather days or line;
//             buttons soft, block or outline; times shows stage durations; illus adds spot illustrations
// motion      enter, tick, box, check, push as { dur, ease } or { spring }; enterY, enterScale, tickFrom
// garden3d    form organic, turned or survey; color species or status; camera ortho or perspective;
//             lights, beds, grid and labels, per scheme where colours differ

const SYSTEM = '-apple-system, BlinkMacSystemFont, "SF Pro Text", system-ui, "Segoe UI", Roboto, sans-serif';
const SYSTEM_DISPLAY = '-apple-system, BlinkMacSystemFont, "SF Pro Display", system-ui, "Segoe UI", Roboto, sans-serif';

window.PDM_DIRECTIONS = [
  {
    id: "current",
    name: "Current F0",
    words: "Baseline · as built",
    summary:
      "The palette, type and shapes in Mark's F0 scaffold (app/src/ui/tokens.ts on feat/app-scaffold), unchanged, so every direction can be compared with what exists.",
    philosophy: "Apple-native defaults: iOS text styles in the system font, inset cards, leaf green on warm off-white. Its accent and healthy are nearly the same green, and watch and sick look alike to colour-blind users (philosophies.md 7).",
    references: [{ label: "PR #42", url: "https://github.com/aacruzgon/GreenLens/pull/42" }],
    type: "System font throughout, iOS text styles 34 to 12 pt",
    motionNote: "Fades of about 250 ms, the spinner and the check; no springs.",
    garden3dNote: "Organic plant shapes in species greens, with a status ring under each plant; slow auto-orbit.",
    colors: {
      light: {
        background: "#F6F7F2", surface: "#FFFFFF", surfaceMuted: "#EEF1E8", border: "#D9DFD2",
        text: "#18211C", textMuted: "#56625A", accent: "#2F6B47", accentText: "#FFFFFF", accentSoft: "#E1EEE4",
        danger: "#B3261E", dangerSoft: "#F9E2DF", info: "#285D86", infoSoft: "#E1ECF5",
        healthy: "#26704A", healthySoft: "#E0F0E6", watch: "#8A5300", watchSoft: "#FBEBD0",
        sick: "#B3261E", sickSoft: "#F9E2DF", dead: "#5C6360", deadSoft: "#E7E9E8",
      },
      dark: {
        background: "#0F1412", surface: "#18201C", surfaceMuted: "#212A25", border: "#2F3A34",
        text: "#E7EDE9", textMuted: "#A2AEA6", accent: "#7CC59A", accentText: "#0D1A12", accentSoft: "#1F3328",
        danger: "#F2948B", dangerSoft: "#3A1F1C", info: "#8EC1EA", infoSoft: "#1B2B38",
        healthy: "#79D19D", healthySoft: "#1C3326", watch: "#F2BE5C", watchSoft: "#3A2E17",
        sick: "#F2948B", sickSoft: "#3A1F1C", dead: "#A7AEAA", deadSoft: "#2A2F2D",
      },
    },
    fonts: { display: SYSTEM_DISPLAY, body: SYSTEM, num: "inherit", displayWeight: 700, displayTracking: "0", google: [] },
    sections: "title", sectionFont: "display",
    shape: { card: 16, button: 12, chip: 999, photo: 16, check: "circle", cardStyle: "border", iconStroke: 1.8 },
    layout: { home: "large", plants: "grid", scan: "bar", region: "box", unsure: "plates", weather: "days", buttons: "soft", times: false, illus: false },
    motion: {
      enter: { dur: 250, ease: "ease-out" }, enterY: 10, enterScale: 1, tickFrom: 1,
      tick: { dur: 200, ease: "ease-out" },
      box: { dur: 400, ease: "ease-out" },
      check: { dur: 200, ease: "ease-out" },
      push: { dur: 350, ease: "cubic-bezier(.2,.8,.2,1)" },
    },
    garden3d: {
      form: "organic", color: "species", ortho: false, edges: false, shadows: true, grid: false, autoRotate: true, scaleBar: false,
      roughness: 0.9, beds: "wood",
      lights: { hemi: 0.78, hemiDark: 0.5, sun: "#FFFFFF", sunI: 0.95, sunPos: [3.5, 6, 2.5] },
      light: { bg: "#F6F7F2", ground: "#DCE3D2", base: "#C9D2BD", wood: "#A98A62", soil: "#5B4636", pot: "#B8643C" },
      dark: { bg: "#0F1412", ground: "#1E2722", base: "#16201B", wood: "#6E5A40", soil: "#3A2E24", pot: "#8A4A2C" },
      foliage: { tomato: "#4E8A3C", pepper: "#3F7D3A", basil: "#6DB24C", lettuce: "#9BCB6A", kale: "#3F6B5C" },
      fruit: { tomato: "#D2452F", pepper: "#E07B22" },
    },
  },

  {
    id: "herbarium",
    name: "Herbarium",
    words: "Restrained · precise · considered",
    summary:
      "A pressed specimen sheet: the plant is the colour, the interface is paper and ink. Field notebook refined, through Swiss/Rams, the field guide and calm technology. The one green direction, in a deep blue-green that never reads as \"healthy\".",
    philosophy: "Quiet and exact. Stages tick in ink, the box draws like a pencil outline, \"Not sure yet\" is set in italics over two equal plates, and the closing shot is the garden as a white architect's model with one rust-red plant.",
    references: [
      { label: "philosophies.md 7.1", url: "../philosophies.md#71-herbarium" },
      { label: "Newsreader", url: "https://github.com/google/fonts/tree/main/ofl/newsreader" },
    ],
    type: "Newsreader 600 for content heroes (plant name, diagnosis, plan title), 400 Italic for \"Not sure yet\" and captions; the system font for body and all chrome",
    motionNote: "Page in 240 ms; ink tick 200 ms; specimen outline 480 ms; struck through 220 ms. One no-overshoot spring (380/40) for panels.",
    garden3dNote: "The architect's model: paper-white beds, turned plant forms tinted by status, ink outlines, orthographic camera, north arrow and 1 m scale bar.",
    colors: {
      light: {
        background: "#F5F4EE", surface: "#FFFFFF", surfaceMuted: "#ECEBE3", border: "#D5D3C7",
        text: "#191E1A", textMuted: "#545C55", accent: "#2C5C3E", accentText: "#FFFFFF", accentSoft: "#E1EBE2",
        danger: "#A8201A", dangerSoft: "#F8E2DE", info: "#285A80", infoSoft: "#E1EAF2",
        healthy: "#28683F", healthySoft: "#E0EEE4", watch: "#7C4D00", watchSoft: "#F7EACF",
        sick: "#B0173A", sickSoft: "#F9E1E5", dead: "#56636B", deadSoft: "#E4E7E8",
      },
      dark: {
        background: "#101412", surface: "#181D1A", surfaceMuted: "#212723", border: "#333B36",
        text: "#E8ECE6", textMuted: "#A5AEA7", accent: "#8BC69F", accentText: "#0E1A12", accentSoft: "#1E3127",
        danger: "#F59BA0", dangerSoft: "#3B1D20", info: "#94C3E8", infoSoft: "#1A2A37",
        healthy: "#8AD6C0", healthySoft: "#173329", watch: "#EDBF62", watchSoft: "#382D16",
        sick: "#F3A383", sickSoft: "#3A2219", dead: "#A9AFAB", deadSoft: "#2A2E2C",
      },
    },
    fonts: {
      display: '"Newsreader", "Iowan Old Style", Georgia, serif', body: SYSTEM, num: "inherit",
      displayWeight: 600, displayTracking: "-0.005em",
      google: ["Newsreader:ital,opsz,wght@0,6..72,400;0,6..72,600;1,6..72,400"],
    },
    sections: "title", sectionFont: "body", italicUnsure: true,
    shape: { card: 10, button: 6, chip: 999, photo: 0, check: "square", cardStyle: "border", iconStroke: 1.6 },
    layout: { home: "large", plants: "rows", scan: "edge", region: "ink", unsure: "plates", weather: "days", buttons: "soft", times: false, illus: false },
    motion: {
      enter: { dur: 240, ease: "cubic-bezier(0.2, 0, 0, 1)" }, enterY: 6, enterScale: 1, tickFrom: 1,
      tick: { dur: 200, ease: "cubic-bezier(0.3, 0, 0.2, 1)" },
      box: { dur: 480, ease: "cubic-bezier(0.4, 0, 0.2, 1)" },
      check: { dur: 220, ease: "ease-out" },
      push: { dur: 350, ease: "cubic-bezier(.2,.8,.2,1)" },
    },
    garden3d: {
      form: "turned", color: "status", desaturate: 0.3, ortho: true, edges: true, shadows: true, grid: false, autoRotate: false, scaleBar: true,
      roughness: 0.95, beds: "paper", edgeOpacity: 0.85,
      lights: { hemi: 0.75, hemiDark: 0.55, hemiGround: "#D8D2C2", sun: "#FFFFFF", sunI: 0.75, sunPos: [-3.5, 5, 3.5] },
      light: { bg: "#F5F4EE", ground: "#F5F4EE", base: "#E6E2D6", wood: "#ECE7DA", soil: "#E2DCCD", pot: "#ECE7DA" },
      dark: { bg: "#101412", ground: "#151a17", base: "#1A1F1C", wood: "#2B312D", soil: "#232925", pot: "#2B312D" },
      foliage: { tomato: "#4E8A3C", pepper: "#3F7D3A", basil: "#6DB24C", lettuce: "#9BCB6A", kale: "#3F6B5C" },
      fruit: { tomato: "#D2452F", pepper: "#E07B22" },
    },
  },

  {
    id: "seedpacket",
    name: "Seed Packet",
    words: "Warm · crafted · generous",
    summary:
      "An heirloom seed catalogue brought to life: printed colour, a soft serif with character, things that grow when they appear. Beet plum, a garden colour that is none of the status hues, used flat and never in a gradient.",
    philosophy: "Warm where the news is good, still where it isn't: the diagnosis card drops the bounce and the illustrations, and \"Not sure yet\" sits in Fraunces Italic over two equal packets. The closing shot is a clay diorama growing up out of the table.",
    references: [
      { label: "philosophies.md 7.2", url: "../philosophies.md#72-seed-packet" },
      { label: "Fraunces", url: "https://github.com/undercasetype/Fraunces" },
    ],
    type: "Fraunces 600 for content heroes and plant names on cards, 400 Italic for asides; the system font for body and all chrome",
    motionNote: "Packet open: spring 260/20 with 8% overshoot; sprout tick: spring 500/18; lasso 560 ms then a 1.04 pulse; stamp: press 0.92 and spring back.",
    garden3dNote: "The clay diorama: wooden slabs with soil tops, lumpy plants in printed status tones, a warm low key light with long shadows, on a cream table.",
    colors: {
      light: {
        background: "#FAF3E4", surface: "#FFFCF6", surfaceMuted: "#F2E7D0", border: "#E0D0B0",
        text: "#2A1D12", textMuted: "#66523F", accent: "#7D2B63", accentText: "#FFFFFF", accentSoft: "#F5E3EF",
        danger: "#A8201A", dangerSoft: "#FBE3DE", info: "#1F5E7A", infoSoft: "#DFEEF3",
        healthy: "#1F6A3A", healthySoft: "#DFF0E2", watch: "#7E4A00", watchSoft: "#FBE9C6",
        sick: "#B0173A", sickSoft: "#FCE2E6", dead: "#56636B", deadSoft: "#E8E6E1",
      },
      dark: {
        background: "#1B1511", surface: "#251D17", surfaceMuted: "#30261E", border: "#46382C",
        text: "#F6ECDF", textMuted: "#C4B39E", accent: "#E9B0DA", accentText: "#2A0D22", accentSoft: "#3C2036",
        danger: "#FF9F8A", dangerSoft: "#431F19", info: "#8FCBE3", infoSoft: "#173040",
        healthy: "#A6D98A", healthySoft: "#22331A", watch: "#F5C267", watchSoft: "#3D2C12",
        sick: "#F590B5", sickSoft: "#43202E", dead: "#9C958B", deadSoft: "#302B26",
      },
    },
    fonts: {
      display: '"Fraunces", "Iowan Old Style", Georgia, serif', body: SYSTEM, num: "inherit",
      displayWeight: 600, displayTracking: "-0.01em",
      google: ["Fraunces:ital,opsz,wght@0,9..144,400;0,9..144,600;1,9..144,400"],
    },
    sections: "title", sectionFont: "display", sectionSize: 22, italicUnsure: true, heroNames: true,
    shape: { card: 16, button: 12, chip: 999, photo: 16, check: "circle", cardStyle: "block", iconStroke: 2, mat: true },
    layout: { home: "large", plants: "grid", scan: "bar", region: "lasso", unsure: "plates", weather: "days", buttons: "block", times: false, illus: true },
    motion: {
      enter: { spring: { stiffness: 260, damping: 20, mass: 1 } }, enterY: 16, enterScale: 0.98, tickFrom: 0.6,
      tick: { spring: { stiffness: 500, damping: 18, mass: 1 } },
      box: { dur: 560, ease: "cubic-bezier(0.65, 0, 0.35, 1)" },
      check: { spring: { stiffness: 420, damping: 16, mass: 1 } },
      push: { dur: 350, ease: "cubic-bezier(.2,.8,.2,1)" },
    },
    garden3d: {
      form: "organic", color: "status", ortho: false, fov: 30, edges: false, shadows: true, grid: false, autoRotate: false, scaleBar: false,
      roughness: 0.8, beds: "wood",
      lights: { hemi: 0.35, hemiDark: 0.25, sun: "#FFE6C4", sunI: 1.2, sunPos: [0.5, 2.6, 6], fill: "#C9DDF2", fillI: 0.3 },
      light: { bg: "#FAF3E4", ground: "#F2E7D0", base: "#E0D0B0", wood: "#B98A5E", soil: "#6B4A32", pot: "#C0673D" },
      dark: { bg: "#1B1511", ground: "#251D17", base: "#30261E", wood: "#7A5A3C", soil: "#4A3424", pot: "#8A4A2C" },
      foliage: { tomato: "#4E8A3C", pepper: "#3F7D3A", basil: "#6DB24C", lettuce: "#9BCB6A", kale: "#3F6B5C" },
      fruit: { tomato: "#D2452F", pepper: "#E07B22" },
    },
  },

  {
    id: "instrument",
    name: "Instrument",
    words: "Exact · legible · candid",
    summary:
      "A field instrument that shows its working: black controls, white paper, colour only where a measurement or a status lives. No brand hue at all, so every coloured mark on screen means something.",
    philosophy: "Real engineering, honest about its limits. The analysis is a run log with times in mono, the reticle locks onto the lesion with its size in millimetres, \"Not sure yet\" is a two-column comparison of the marks that separate the candidates, and the 3D garden is a survey model with every height labelled.",
    references: [
      { label: "philosophies.md 7.3", url: "../philosophies.md#73-instrument" },
      { label: "Atkinson Hyperlegible", url: "https://github.com/google/fonts/tree/main/ofl/atkinsonhyperlegiblenext" },
    ],
    type: "Atkinson Hyperlegible Next for all content text, Atkinson Hyperlegible Mono for measurements and units; chrome stays SF Pro",
    motionNote: "Cut: opacity only, 180 ms; readout: critically damped spring (600/49); reticle: brackets slide in over 260 ms; log: 120 ms fill.",
    garden3dNote: "The survey model: a 0.5 m grid, beds as outlines, height-true cylinders with canopy discs in the status colour, height labels, flat light and no shadows.",
    colors: {
      light: {
        background: "#FFFFFF", surface: "#FFFFFF", surfaceMuted: "#F1F3F2", border: "#C8CECB",
        text: "#0A0F0C", textMuted: "#454E49", accent: "#111814", accentText: "#FFFFFF", accentSoft: "#E5E9E7",
        danger: "#B02A00", dangerSoft: "#FDE8DF", info: "#0050A0", infoSoft: "#E3EEFA",
        healthy: "#006B3C", healthySoft: "#DDF2E6", watch: "#8A4B00", watchSoft: "#FFEDD2",
        sick: "#B3123A", sickSoft: "#FDE4E9", dead: "#56636B", deadSoft: "#E6E9EB",
      },
      dark: {
        background: "#0B0E0D", surface: "#141917", surfaceMuted: "#1C2220", border: "#2F3734",
        text: "#F2F5F3", textMuted: "#A9B3AE", accent: "#EEF2F0", accentText: "#0B0E0D", accentSoft: "#27302C",
        danger: "#FF9466", dangerSoft: "#3E1E12", info: "#7DB8FF", infoSoft: "#13253B",
        healthy: "#8AD6C0", healthySoft: "#12312A", watch: "#FFB547", watchSoft: "#3A2810",
        sick: "#FF7F6E", sickSoft: "#3E1A17", dead: "#8E9692", deadSoft: "#262C2A",
      },
    },
    fonts: {
      display: '"Atkinson Hyperlegible Next", system-ui, sans-serif', body: '"Atkinson Hyperlegible Next", system-ui, sans-serif',
      num: '"Atkinson Hyperlegible Mono", ui-monospace, Menlo, monospace',
      displayWeight: 700, displayTracking: "-0.01em",
      google: ["Atkinson+Hyperlegible+Next:wght@400;600;700", "Atkinson+Hyperlegible+Mono:wght@500"],
    },
    sections: "label", sectionFont: "body",
    capture: { accent: "#FFFFFF", accentText: "#0B0E0D" },
    shape: { card: 8, button: 8, chip: 6, photo: 0, check: "square", cardStyle: "rule", iconStroke: 1.8 },
    layout: { home: "large", plants: "rows", scan: "edge", region: "reticle", unsure: "table", weather: "line", buttons: "outline", times: true, illus: false },
    motion: {
      enter: { dur: 180, ease: "cubic-bezier(0, 0, 0.2, 1)" }, enterY: 0, enterScale: 1, tickFrom: 1,
      tick: { spring: { stiffness: 600, damping: 49, mass: 1 } },
      box: { dur: 260, ease: "cubic-bezier(0.2, 0, 0, 1)" },
      check: { dur: 120, ease: "linear" },
      push: { dur: 350, ease: "cubic-bezier(.2,.8,.2,1)" },
    },
    garden3d: {
      form: "survey", color: "status", emissive: 0.15, ortho: true, edges: true, shadows: false, grid: true, autoRotate: false, scaleBar: true,
      roughness: 1, beds: "outline", labelHeights: true, edgeOpacity: 0.9,
      lights: { hemi: 1.0, hemiDark: 0.9, sun: "#FFFFFF", sunI: 0, sunPos: [0, 6, 0] },
      light: { bg: "#FFFFFF", ground: "#FFFFFF", base: "#F1F3F2", wood: "#0A0F0C", soil: "#F1F3F2", pot: "#0A0F0C" },
      dark: { bg: "#0B0E0D", ground: "#0B0E0D", base: "#141917", wood: "#F2F5F3", soil: "#1C2220", pot: "#F2F5F3" },
      foliage: { tomato: "#4E8A3C", pepper: "#3F7D3A", basil: "#6DB24C", lettuce: "#9BCB6A", kale: "#3F6B5C" },
      fruit: { tomato: "#D2452F", pepper: "#E07B22" },
    },
  },
];
