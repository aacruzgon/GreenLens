// A round bar button over a photo (design 3.1: Back and In the garden float over the diagnosis's photo as clear
// glass bar buttons). Clear Liquid Glass where iOS draws it; elsewhere, and with Reduce Transparency on, the
// mockup's white disc with a hairline (styles.css .glass-btn). White ink: it sits on the photo's dark scrim.
import type { ReactNode } from "react";
import { Pressable, View } from "react-native";
import { GlassView } from "expo-glass-effect";

import { useGlass } from "../glass";
import { minTouch } from "../tokens";

export interface BarButtonProps {
  accessibilityLabel: string;
  onPress: () => void;
  /** The icon, drawn in white. */
  children: ReactNode;
  testID?: string;
}

/** The ink on a bar button over a photo. */
export const BAR_INK = "#FFFFFF";

export function BarButton({ accessibilityLabel, onPress, children, testID }: BarButtonProps) {
  const glass = useGlass();
  const disc = { width: minTouch, height: minTouch, borderRadius: minTouch / 2, alignItems: "center", justifyContent: "center" } as const;
  return (
    <Pressable accessibilityRole="button" accessibilityLabel={accessibilityLabel} onPress={onPress} testID={testID} style={({ pressed }) => ({ opacity: pressed ? 0.7 : 1 })}>
      {glass ? (
        <GlassView glassEffectStyle="clear" isInteractive style={disc}>
          {children}
        </GlassView>
      ) : (
        <View style={[disc, { backgroundColor: "rgba(255,255,255,0.16)", borderWidth: 1, borderColor: "rgba(255,255,255,0.28)" }]}>{children}</View>
      )}
    </Pressable>
  );
}
