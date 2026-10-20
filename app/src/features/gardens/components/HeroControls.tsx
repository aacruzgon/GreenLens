// The hero's top row (screens 3.1; styles.css .hero-top): the garden switcher (name and a chevron) at the
// left and **+** (new plant) at the right, as round neutral controls. Over a photo they are translucent dark
// with white ink; on the wash they are solid surface. Never tinted (design 2, 3.1).
import { ActionSheetIOS, Alert, Platform, Pressable, View } from "react-native";

import { Text, gutter, minTouch, useTheme } from "@/ui";

import type { GardenChoice } from "../model/views";
import { Icon } from "./Icons";

export interface HeroControlsProps {
  gardens: GardenChoice[];
  activeId: string;
  /** Undefined hides **+** (viewers). */
  onAddPlant?: () => void;
  onPickGarden: (id: string) => void;
  /** True over the hero photo. */
  overPhoto: boolean;
  /** The status bar's height. */
  topInset: number;
}

/** Lists the gardens in the platform's own sheet. */
function chooseGarden(gardens: GardenChoice[], onPick: (id: string) => void) {
  if (Platform.OS === "ios") {
    ActionSheetIOS.showActionSheetWithOptions(
      { options: [...gardens.map((g) => g.name), "Cancel"], cancelButtonIndex: gardens.length },
      (i) => {
        const picked = gardens[i];
        if (picked) onPick(picked.id);
      },
    );
    return;
  }
  Alert.alert("Switch garden", undefined, [...gardens.map((g) => ({ text: g.name, onPress: () => onPick(g.id) })), { text: "Cancel", style: "cancel" as const }]);
}

export function HeroControls({ gardens, activeId, onAddPlant, onPickGarden, overPhoto, topInset }: HeroControlsProps) {
  const { colors } = useTheme();
  const active = gardens.find((g) => g.id === activeId);
  const switchable = gardens.length >= 2;
  const fill = overPhoto ? "rgba(0,0,0,0.38)" : colors.surface;
  const ink = overPhoto ? "#FFFFFF" : colors.text;
  const pill = { minHeight: minTouch, borderRadius: minTouch / 2, backgroundColor: fill, alignItems: "center" as const, justifyContent: "center" as const };

  return (
    <View
      pointerEvents="box-none"
      testID="hero-controls"
      style={{
        position: "absolute",
        top: topInset + 4,
        left: 0,
        right: 0,
        paddingHorizontal: gutter,
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 12,
      }}
    >
      {active ? (
        <Pressable
          accessibilityRole={switchable ? "button" : "text"}
          accessibilityLabel={switchable ? `Garden: ${active.name}. Switch garden` : `Garden: ${active.name}`}
          disabled={!switchable}
          onPress={() => chooseGarden(gardens, onPickGarden)}
          style={[pill, { flexDirection: "row", gap: 6, paddingHorizontal: 16, flexShrink: 1 }]}
          testID="garden-switcher"
        >
          <Text variant="subheadline" numberOfLines={1} style={{ color: ink, flexShrink: 1 }}>
            {active.name}
          </Text>
          {switchable ? <Icon name="chevron" color={ink} size={16} /> : null}
        </Pressable>
      ) : (
        <View />
      )}
      {onAddPlant ? (
        <Pressable accessibilityRole="button" accessibilityLabel="Add a plant" onPress={onAddPlant} style={[pill, { width: minTouch }]} testID="add-plant">
          <Icon name="plus" color={ink} size={22} />
        </Pressable>
      ) : null}
    </View>
  );
}
