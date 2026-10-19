// docs/frontend 14: colour roles, a 4-point spacing scale, radii and type sizes, in the Conservatory
// direction (docs/design/design.md 3.1): F0's palette unchanged plus `wash`, Urbanist for content heroes,
// large continuous radii and slow, damped motion.
// Light and dark share role names; components read roles through useTheme(), never raw colours.
// Contrast: text on surface and accent text on accent are at least 4.5:1 in both schemes (WCAG 2.2 AA).

export interface ColorRoles {
  background: string;
  surface: string;
  surfaceMuted: string;
  border: string;
  text: string;
  textMuted: string;
  accent: string;
  accentText: string;
  accentSoft: string;
  danger: string;
  dangerSoft: string;
  info: string;
  infoSoft: string;
  healthy: string;
  healthySoft: string;
  watch: string;
  watchSoft: string;
  sick: string;
  sickSoft: string;
  dead: string;
  deadSoft: string;
  overlay: string;
  /** The pale sage behind hero photos and at the top of every screen (design 3.1, the melt). */
  wash: string;
}

export const light: ColorRoles = {
  background: "#F6F7F2",
  surface: "#FFFFFF",
  surfaceMuted: "#EEF1E8",
  border: "#D9DFD2",
  text: "#18211C",
  textMuted: "#56625A",
  accent: "#2F6B47",
  accentText: "#FFFFFF",
  accentSoft: "#E1EEE4",
  danger: "#B3261E",
  dangerSoft: "#F9E2DF",
  info: "#285D86",
  infoSoft: "#E1ECF5",
  healthy: "#26704A",
  healthySoft: "#E0F0E6",
  watch: "#8A5300",
  watchSoft: "#FBEBD0",
  sick: "#B3261E",
  sickSoft: "#F9E2DF",
  dead: "#5C6360",
  deadSoft: "#E7E9E8",
  overlay: "rgba(14, 20, 17, 0.45)",
  wash: "#E3EBDC",
};

export const dark: ColorRoles = {
  background: "#0F1412",
  surface: "#18201C",
  surfaceMuted: "#212A25",
  border: "#2F3A34",
  text: "#E7EDE9",
  textMuted: "#A2AEA6",
  accent: "#7CC59A",
  accentText: "#0D1A12",
  accentSoft: "#1F3328",
  danger: "#F2948B",
  dangerSoft: "#3A1F1C",
  info: "#8EC1EA",
  infoSoft: "#1B2B38",
  healthy: "#79D19D",
  healthySoft: "#1C3326",
  watch: "#F2BE5C",
  watchSoft: "#3A2E17",
  sick: "#F2948B",
  sickSoft: "#3A1F1C",
  dead: "#A7AEAA",
  deadSoft: "#2A2F2D",
  overlay: "rgba(0, 0, 0, 0.6)",
  wash: "#18231D",
};

export const space = { xxs: 2, xs: 4, sm: 8, md: 12, lg: 16, xl: 24, xxl: 32, xxxl: 48 } as const;

// Cards and photos at 28 pt, buttons and chips as pills (design 3.1).
export const radius = { sm: 8, md: 12, lg: 16, xl: 24, card: 28, pill: 999 } as const;

/** Cards: a two-layer warm shadow in light mode, a border in dark (design 3.1). */
export const shadow = {
  card: { boxShadow: "0 1px 1px rgba(20, 33, 28, 0.04), 0 18px 40px -18px rgba(20, 33, 28, 0.22)" },
  pill: { boxShadow: "0 18px 36px -16px rgba(20, 14, 10, 0.55)" },
  soft: { boxShadow: "0 1px 2px rgba(20, 14, 10, 0.08)" },
} as const;

// Apple's default text styles (HIG typography, iOS sizes), scaled by the system text size.
export const type = {
  largeTitle: { fontSize: 34, lineHeight: 41, fontWeight: "700" },
  title1: { fontSize: 28, lineHeight: 34, fontWeight: "700" },
  title2: { fontSize: 22, lineHeight: 28, fontWeight: "700" },
  title3: { fontSize: 20, lineHeight: 25, fontWeight: "600" },
  headline: { fontSize: 17, lineHeight: 22, fontWeight: "600" },
  body: { fontSize: 17, lineHeight: 22, fontWeight: "400" },
  callout: { fontSize: 16, lineHeight: 21, fontWeight: "400" },
  subhead: { fontSize: 15, lineHeight: 20, fontWeight: "400" },
  footnote: { fontSize: 13, lineHeight: 18, fontWeight: "400" },
  caption: { fontSize: 12, lineHeight: 16, fontWeight: "400" },
  // The display face, for content heroes only and never below 22 pt (design 3.1): Urbanist 600 at
  // -0.03 em. Bars, body and numbers stay in the system font.
  hero: { fontSize: 54, lineHeight: 52, fontFamily: "Urbanist_600SemiBold", letterSpacing: -1.6 },
  display: { fontSize: 40, lineHeight: 44, fontFamily: "Urbanist_600SemiBold", letterSpacing: -1.2 },
  displaySmall: { fontSize: 22, lineHeight: 26, fontFamily: "Urbanist_600SemiBold", letterSpacing: -0.66 },
} as const;

// Slow and damped, never bouncy (design 3.1). Reduced motion drops springs and slides for fades.
export const motion = {
  /** Screens and sections enter on a critically damped spring, rising 24 pt. */
  enter: { stiffness: 170, damping: 26, mass: 1, rise: 24, stagger: 60 },
  /** A checked task. */
  check: { stiffness: 300, damping: 30, mass: 1 },
  /** Stage ticks. */
  tick: { duration: 300 },
  /** The hotspot settles, then breathes. */
  hotspot: { duration: 520, breathe: 2600 },
  /** Pushes and sheets: the iOS sheet curve. */
  push: { duration: 420, bezier: [0.32, 0.72, 0, 1] },
} as const;

export type TypeVariant = keyof typeof type;

/** The minimum touch target, in points (docs/frontend 14; HIG accessibility). */
export const MIN_TOUCH = 44;
