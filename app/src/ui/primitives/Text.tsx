// Text in the iOS text styles (system font) or a display style (Urbanist, 22 pt and above; design.md 3.1).
// Every size scales with Dynamic Type: allowFontScaling stays on.
import { Text as RNText, type TextProps as RNTextProps, type TextStyle } from "react-native";

import { displayFamily, useDisplayFontLoaded } from "../fonts";
import { useTheme } from "../theme";
import { displayStyles, textStyles, type ColorRole, type DisplayStyleName, type TextStyleName } from "../tokens";

export type TextVariant = TextStyleName | DisplayStyleName;

export interface TextProps extends RNTextProps {
  /** An iOS text style, or a display style for content heroes. Defaults to body. */
  variant?: TextVariant;
  /** A colour role. Defaults to text. */
  color?: ColorRole;
  /** Semibold, for emphasis inside a text style. */
  emphasis?: boolean;
  align?: TextStyle["textAlign"];
}

function isDisplay(variant: TextVariant): variant is DisplayStyleName {
  return variant in displayStyles;
}

function DisplayText({ variant, style, ...rest }: RNTextProps & { variant: DisplayStyleName }) {
  const loaded = useDisplayFontLoaded();
  const family = displayFamily(loaded);
  const d = displayStyles[variant];
  return (
    <RNText
      {...rest}
      style={[
        d,
        // Urbanist carries its own weight; without it the system font stands in at 600.
        family ? { fontFamily: family } : { fontWeight: "600" },
        style,
      ]}
    />
  );
}

export function Text({ variant = "body", color = "text", emphasis, align, style, accessibilityRole, ...rest }: TextProps) {
  const { colors } = useTheme();
  const base: TextStyle = { color: colors[color], textAlign: align };
  if (isDisplay(variant)) {
    return <DisplayText variant={variant} accessibilityRole={accessibilityRole ?? "header"} {...rest} style={[base, style]} />;
  }
  const t = textStyles[variant];
  const header = variant === "largeTitle" || variant.startsWith("title");
  return (
    <RNText
      {...rest}
      accessibilityRole={accessibilityRole ?? (header ? "header" : undefined)}
      style={[t, emphasis ? { fontWeight: "600" } : null, base, style]}
    />
  );
}
