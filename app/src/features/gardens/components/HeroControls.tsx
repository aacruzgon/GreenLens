// The hero's top row (styles.css .hero-top: 14 pt from the right, just under the status bar): **+** (new
// plant) as a 44 pt round glass control. Over a photo it is a white glass disc with a hairline and white ink
// (.glass-btn); on the wash it is solid surface. Never tinted (design 2, 3.1). The 3D garden's cube joins in F7.
// The garden switcher is not here: it is the ▾ on the hero's title, with two or more gardens (chooseGarden).
import { ActionSheetIOS, Alert, Platform, Pressable, View } from "react-native";

import { minTouch, useTheme } from "@/ui";

import type { GardenChoice } from "../model/views";
import { Icon } from "./Icons";

export interface HeroControlsProps {
  /** Undefined hides **+** (viewers). */
  onAddPlant?: () => void;
  /** True over the hero photo. */
  overPhoto: boolean;
  /** The status bar's height. */
  topInset: number;
}

/** Lists the gardens in the platform's own sheet. */
export function chooseGarden(gardens: GardenChoice[], onPick: (id: string) => void) {
  if (Platform.OS === "ios") {
    ActionSheetIOS.showActionSheetWithOptions(
      { title: "Switch garden", options: [...gardens.map((g) => g.name), "Cancel"], cancelButtonIndex: gardens.length },
      (i) => {
        const picked = gardens[i];
        if (picked) onPick(picked.id);
      },
    );
    return;
  }
  Alert.alert("Switch garden", undefined, [...gardens.map((g) => ({ text: g.name, onPress: () => onPick(g.id) })), { text: "Cancel", style: "cancel" as const }]);
}

export function HeroControls({ onAddPlant, overPhoto, topInset }: HeroControlsProps) {
  const { colors } = useTheme();
  if (!onAddPlant) return null;
  const ink = overPhoto ? "#FFFFFF" : colors.text;

  return (
    <View
      pointerEvents="box-none"
      testID="hero-controls"
      style={{ position: "absolute", top: Math.max(topInset - 2, 8), right: 14, flexDirection: "row", gap: 8 }}
    >
      <Pressable
        accessibilityRole="button"
        accessibilityLabel="Add a plant"
        onPress={onAddPlant}
        testID="add-plant"
        style={({ pressed }) => ({
          width: minTouch,
          height: minTouch,
          borderRadius: minTouch / 2,
          alignItems: "center",
          justifyContent: "center",
          backgroundColor: overPhoto ? "rgba(255,255,255,0.16)" : colors.surface,
          borderWidth: overPhoto ? 1 : 0,
          borderColor: "rgba(255,255,255,0.28)",
          opacity: pressed ? 0.7 : 1,
        })}
      >
        <Icon name="plus" color={ink} size={22} />
      </Pressable>
    </View>
  );
}
