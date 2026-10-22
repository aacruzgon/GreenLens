// "Check your soil" on the live analysis's More button (screens 3.3): the squeeze and jar tests as a menu. On iOS it
// is SwiftUI's Menu (HIG Menus; hig/expo.md): it opens from the button. Elsewhere, an alert with the two choices.
import { Button as MenuItem, Host, Image as Symbol, Menu } from "@expo/ui/swift-ui";
import { accessibilityLabel, frame } from "@expo/ui/swift-ui/modifiers";
import { Alert, Platform, View } from "react-native";

import type { GuidedKind } from "@/domain/types";
import { IconButton, camera, minTouch } from "@/ui";

import { ScanIcon } from "./ScanIcons";

const SIZE = minTouch + 4;
const circle = { width: SIZE, height: SIZE, borderRadius: SIZE / 2, backgroundColor: camera.card } as const;

export function SoilMenu({ onPick }: { onPick: (kind: GuidedKind) => void }) {
  if (Platform.OS === "ios") {
    return (
      <View style={[circle, { alignItems: "center", justifyContent: "center" }]} testID="soil-menu">
        <Host matchContents colorScheme="dark">
          <Menu label={<Symbol systemName="ellipsis" size={20} color={camera.ink} />} modifiers={[frame({ width: SIZE, height: SIZE }), accessibilityLabel("Check your soil")]}>
            <MenuItem label="Squeeze test" systemImage="hand.raised" onPress={() => onPick("squeeze_test")} />
            <MenuItem label="Jar test" systemImage="flask" onPress={() => onPick("jar_test")} />
          </Menu>
        </Host>
      </View>
    );
  }
  const ask = () =>
    Alert.alert("Check your soil", undefined, [
      { text: "Squeeze test", onPress: () => onPick("squeeze_test") },
      { text: "Jar test", onPress: () => onPick("jar_test") },
      { text: "Cancel", style: "cancel" },
    ]);
  return <IconButton accessibilityLabel="Check your soil" onPress={ask} icon={() => <ScanIcon name="more" color={camera.ink} />} style={circle} />;
}
