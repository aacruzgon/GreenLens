// The bar of a screen pushed from Settings: Back and the title, solid like the content (design 6: glass only in the
// system's chrome). The connectors and chat screens draw the same bar.
import { View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { IconButton, LineIcon, Text, space, useTheme } from "@/ui";

export function BackHeader({ title, onBack }: { title: string; onBack: () => void }) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  return (
    <View
      style={{
        paddingTop: insets.top + space[1],
        paddingHorizontal: space[2],
        paddingBottom: space[2],
        flexDirection: "row",
        alignItems: "center",
        gap: space[1],
        borderBottomWidth: 1,
        borderBottomColor: colors.border,
        backgroundColor: colors.background,
      }}
    >
      <IconButton accessibilityLabel="Back" onPress={onBack} icon={({ color }) => <LineIcon name="back" color={color} size={22} />} />
      <Text variant="headline" accessibilityRole="header" style={{ flex: 1 }} numberOfLines={1}>
        {title}
      </Text>
    </View>
  );
}
