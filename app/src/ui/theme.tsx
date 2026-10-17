import { createContext, useContext, useMemo, type ReactNode } from "react";
import { useColorScheme } from "react-native";

import { dark, light, radius, space, type, type ColorRoles } from "./tokens";

export interface Theme {
  scheme: "light" | "dark";
  colors: ColorRoles;
  space: typeof space;
  radius: typeof radius;
  type: typeof type;
}

const ThemeContext = createContext<Theme | null>(null);

export function ThemeProvider({ children, scheme: forced }: { children: ReactNode; scheme?: "light" | "dark" }) {
  const system = useColorScheme();
  const scheme = forced ?? (system === "dark" ? "dark" : "light");
  const theme = useMemo<Theme>(
    () => ({ scheme, colors: scheme === "dark" ? dark : light, space, radius, type }),
    [scheme],
  );
  return <ThemeContext.Provider value={theme}>{children}</ThemeContext.Provider>;
}

/** Outside a provider (tests, previews) the light theme is used. */
export function useTheme(): Theme {
  return useContext(ThemeContext) ?? { scheme: "light", colors: light, space, radius, type };
}
