// The theme: colour roles for the current system scheme, plus the reduced-motion setting
// (docs/frontend/README.md 14; design.md 6, every motion has a still version).
import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { AccessibilityInfo, useColorScheme } from "react-native";

import { colors, type ColorRoles, type ColorScheme } from "./tokens";

export interface Theme {
  scheme: ColorScheme;
  dark: boolean;
  colors: ColorRoles;
  reducedMotion: boolean;
}

const ThemeContext = createContext<Theme | null>(null);

/** The system's Reduce Motion setting, kept current while the app runs. */
export function useSystemReducedMotion(): boolean {
  const [reduced, setReduced] = useState(false);
  useEffect(() => {
    let alive = true;
    AccessibilityInfo.isReduceMotionEnabled()
      .then((value) => {
        if (alive) setReduced(value);
      })
      .catch(() => {});
    const sub = AccessibilityInfo.addEventListener("reduceMotionChanged", setReduced);
    return () => {
      alive = false;
      sub.remove();
    };
  }, []);
  return reduced;
}

export interface ThemeProviderProps {
  children: ReactNode;
  /** Forces a scheme; the camera screens are always dark (design 3.1). Defaults to the system's. */
  scheme?: ColorScheme;
  /** Forces reduced motion, for tests and the debug menu. Defaults to the system's setting. */
  reducedMotion?: boolean;
}

export function ThemeProvider({ children, scheme, reducedMotion }: ThemeProviderProps) {
  const system = useColorScheme();
  const systemReduced = useSystemReducedMotion();
  const resolved: ColorScheme = scheme ?? (system === "dark" ? "dark" : "light");
  const theme: Theme = {
    scheme: resolved,
    dark: resolved === "dark",
    colors: colors[resolved],
    reducedMotion: reducedMotion ?? systemReduced,
  };
  return <ThemeContext.Provider value={theme}>{children}</ThemeContext.Provider>;
}

/** The current theme. Outside a ThemeProvider it follows the system scheme with motion on. */
export function useTheme(): Theme {
  const theme = useContext(ThemeContext);
  const system = useColorScheme();
  if (theme) return theme;
  const scheme: ColorScheme = system === "dark" ? "dark" : "light";
  return { scheme, dark: scheme === "dark", colors: colors[scheme], reducedMotion: false };
}

/** True when motion should be replaced by fades. */
export function useReducedMotion(): boolean {
  return useTheme().reducedMotion;
}
