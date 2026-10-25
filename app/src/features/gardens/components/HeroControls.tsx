// The hero's top row (styles.css .hero-top: 14 pt from the right, just under the status bar): **+** (new
// plant) as a 44 pt round glass control. Over a photo it is a white glass disc with a hairline and white ink
// (.glass-btn); on the wash it is solid surface. Never tinted (design 2, 3.1). The cube opens the garden in 3D
// (roadmap F7), for every member.
// The garden switcher is not here: it is the ▾ on the hero's title, with two or more gardens (chooseGarden).
import { ActionSheetIOS, Alert, Platform, Pressable, View } from "react-native";

import { LineIcon, minTouch, useTheme } from "@/ui";

import type { GardenChoice } from "../model/views";
import { Icon } from "./Icons";

export interface HeroControlsProps {
  /** Undefined hides **+** (viewers). */
  onAddPlant?: () => void;
  /** The garden in 3D or on the map. */
  onOpenGarden: () => void;
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

export function HeroControls({ onAddPlant, onOpenGarden, overPhoto, topInset }: HeroControlsProps) {
  const { colors } = useTheme();
  const ink = overPhoto ? "#FFFFFF" : colors.text;
  const disc = ({ pressed }: { pressed: boolean }) => ({
    width: minTouch,
    height: minTouch,
    borderRadius: minTouch / 2,
    alignItems: "center" as const,
    justifyContent: "center" as const,
    backgroundColor: overPhoto ? "rgba(255,255,255,0.16)" : colors.surface,
    borderWidth: overPhoto ? 1 : 0,
    borderColor: "rgba(255,255,255,0.28)",
    opacity: pressed ? 0.7 : 1,
  });

  return (
    <View
      pointerEvents="box-none"
      testID="hero-controls"
      style={{ position: "absolute", top: Math.max(topInset - 2, 8), right: 14, flexDirection: "row", gap: 8 }}
    >
      <Pressable accessibilityRole="button" accessibilityLabel="Garden in 3D" onPress={onOpenGarden} testID="open-garden" style={disc}>
        <LineIcon name="cube" color={ink} size={22} />
      </Pressable>
      {onAddPlant ? (
        <Pressable accessibilityRole="button" accessibilityLabel="Add a plant" onPress={onAddPlant} testID="add-plant" style={disc}>
          <Icon name="plus" color={ink} size={22} />
        </Pressable>
      ) : null}
    </View>
  );
}
