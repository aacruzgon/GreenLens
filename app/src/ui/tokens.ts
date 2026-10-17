// docs/frontend 14: colour roles, a 4-point spacing scale, radii and type sizes.
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
};

export const space = { xxs: 2, xs: 4, sm: 8, md: 12, lg: 16, xl: 24, xxl: 32, xxxl: 48 } as const;

export const radius = { sm: 8, md: 12, lg: 16, xl: 24, pill: 999 } as const;

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
} as const;

export type TypeVariant = keyof typeof type;

/** The minimum touch target, in points (docs/frontend 14; HIG accessibility). */
export const MIN_TOUCH = 44;
