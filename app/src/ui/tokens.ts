// Conservatory's design tokens (docs/design/design.md 3.1, docs/frontend/roadmap.md 2 and 6 item 7).
// Values match docs/design/mockups/directions.js (the `conservatory` direction) value for value.
// Components read colours through useTheme(), never these objects directly.
import type { PlantStatus } from "@/domain/types";

import palette from "./palette.json";

// --- Colour ---------------------------------------------------------------
// F0_COLORS unchanged plus `wash` (design 3.1). The hex values live in palette.json so that
// scripts/check-contrast.mjs reads the same values without a TypeScript build.

export type ColorScheme = "light" | "dark";
export type ColorRoles = typeof palette.light;
export type ColorRole = keyof ColorRoles;

export const colors: Record<ColorScheme, ColorRoles> = palette;

// --- Status (design 6: a word, a colour and a shape) -------------------------
export type { PlantStatus };

export const statusWords: Record<PlantStatus, string> = {
  healthy: "Healthy",
  watch: "Watch",
  sick: "Sick",
  dead: "Dead",
};

export const statusRoles: Record<PlantStatus, { fg: ColorRole; bg: ColorRole }> = {
  healthy: { fg: "healthy", bg: "healthySoft" },
  watch: { fg: "watch", bg: "watchSoft" },
  sick: { fg: "sick", bg: "sickSoft" },
  dead: { fg: "dead", bg: "deadSoft" },
};

// --- Spacing: the 4-point scale ----------------------------------------------
// space[n] is n × 4 pt.
export const space = {
  0: 0,
  0.5: 2,
  1: 4,
  2: 8,
  3: 12,
  4: 16,
  5: 20,
  6: 24,
  7: 28,
  8: 32,
  10: 40,
  12: 48,
  14: 56,
  16: 64,
} as const;
export type SpaceKey = keyof typeof space;

/** The side gutter of every screen. */
export const gutter = space[4];
/** The minimum touch target (design 6, WCAG 2.2). */
export const minTouch = 44;

// --- Radii (directions.js shape) ------------------------------------------------
export const radii = {
  /** Cards (shape.card). */
  card: 28,
  /** Photos (shape.photo). */
  photo: 28,
  /** Pills: buttons and chips (shape.button, shape.chip: 999). */
  pill: 999,
  /** Cards dropping over the camera and the sheet's top corners (styles.css .phase, .g3d-card). */
  sheet: 26,
  /** Text fields and small blocks. */
  field: 14,
  /** Photo thumbnails in a strip (styles.css .pstrip span). */
  thumb: 12,
  small: 8,
} as const;

// --- Elevation ---------------------------------------------------------------
// Cards: a two-layer warm shadow in light mode, a 1 pt border in dark (directions.js shape.shadow, app.js cardStyle "shadow").
// React Native 0.86 takes CSS box-shadow strings in `boxShadow`.
export const elevation = {
  card: {
    light: { boxShadow: "0 1px 1px rgba(20,33,28,.04), 0 18px 40px -18px rgba(20,33,28,.22)", borderWidth: 0 },
    dark: { boxShadow: undefined, borderWidth: 1 },
  },
  /** The large primary pill (styles.css .btn-xl under data-buttons="pill"). Light mode only. */
  pillPrimary: "0 18px 36px -16px rgba(20,14,10,.55)",
  /** The secondary surface pill (styles.css .btn-secondary under data-buttons="pill"). Light mode only. */
  pillSecondary: "0 1px 2px rgba(20,14,10,.08)",
} as const;

// --- Type -------------------------------------------------------------------------
// Body, numbers and chrome: the iOS text styles in the system font (SF Pro), at the Large (default) size.
// Display: Urbanist 600 at −0.03 em, content heroes only, 22 pt and above (design 3.1).

export type TextStyleName =
  | "largeTitle"
  | "title1"
  | "title2"
  | "title3"
  | "headline"
  | "body"
  | "callout"
  | "subheadline"
  | "footnote"
  | "caption1"
  | "caption2";

export type DisplayStyleName = "hero" | "display" | "section";

export interface TextStyleToken {
  fontSize: number;
  lineHeight: number;
  fontWeight: "400" | "600" | "700";
}

// Apple's Dynamic Type sizes at the default (Large) setting: HIG Typography, Specifications.
export const textStyles: Record<TextStyleName, TextStyleToken> = {
  largeTitle: { fontSize: 34, lineHeight: 41, fontWeight: "400" },
  title1: { fontSize: 28, lineHeight: 34, fontWeight: "400" },
  title2: { fontSize: 22, lineHeight: 28, fontWeight: "400" },
  title3: { fontSize: 20, lineHeight: 25, fontWeight: "400" },
  headline: { fontSize: 17, lineHeight: 22, fontWeight: "600" },
  body: { fontSize: 17, lineHeight: 22, fontWeight: "400" },
  callout: { fontSize: 16, lineHeight: 21, fontWeight: "400" },
  subheadline: { fontSize: 15, lineHeight: 20, fontWeight: "400" },
  footnote: { fontSize: 13, lineHeight: 18, fontWeight: "400" },
  caption1: { fontSize: 12, lineHeight: 16, fontWeight: "400" },
  caption2: { fontSize: 11, lineHeight: 13, fontWeight: "400" },
};

/** Urbanist's tracking: −0.03 em (directions.js fonts.displayTracking). */
export const displayTrackingEm = -0.03;

// Sizes from styles.css: .hero-text h1 54/52 (garden name), .profile-head h1 40/44 (diagnosis),
// sectionSize 22 at line height 1.25 (.sec-h h2).
export const displayStyles: Record<DisplayStyleName, { fontSize: number; lineHeight: number; letterSpacing: number }> = {
  hero: { fontSize: 54, lineHeight: 52, letterSpacing: 54 * displayTrackingEm },
  display: { fontSize: 40, lineHeight: 44, letterSpacing: 40 * displayTrackingEm },
  section: { fontSize: 22, lineHeight: 28, letterSpacing: 22 * displayTrackingEm },
};

// --- Motion (directions.js conservatory.motion) ---------------------------------
export const motion = {
  /** Screens and cards enter on a critically damped spring, rising 24 pt and scaling from 0.985. */
  enter: { spring: { stiffness: 170, damping: 26, mass: 1 }, fromY: 24, fromScale: 0.985 },
  /** F1's task check. */
  check: { spring: { stiffness: 300, damping: 30, mass: 1 } },
  /** A stage tick: 300 ms on the sheet curve, scaling from 0.85. */
  tick: { duration: 300, fromScale: 0.85 },
  /** The hotspot settles in 520 ms, then breathes. */
  hotspot: { settle: 520 },
  /** Pushes and sheets: the iOS sheet curve over 420 ms. */
  sheet: { duration: 420, bezier: [0.32, 0.72, 0, 1] as const },
  /** Reduced motion swaps movement for a fade (design 6); 250 ms is F0's fade (directions.js current.motion.enter). */
  fade: { duration: 250 },
} as const;
