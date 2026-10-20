import { NativeTabs } from "expo-router/unstable-native-tabs";

import { useTheme } from "@/ui";

// Native tabs (roadmap F0 item 8). The chosen tab is in the text colour, not green: controls are neutral,
// content is green (design 3.1). Tasks join in F4 and Chat in F6, so no tab is ever empty.
export default function TabsLayout() {
  const { colors } = useTheme();
  return (
    <NativeTabs tintColor={colors.text}>
      <NativeTabs.Trigger name="index">
        <NativeTabs.Trigger.Icon sf={{ default: "leaf", selected: "leaf.fill" }} />
        <NativeTabs.Trigger.Label>Garden</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="settings">
        <NativeTabs.Trigger.Icon sf={{ default: "gearshape", selected: "gearshape.fill" }} />
        <NativeTabs.Trigger.Label>Settings</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
    </NativeTabs>
  );
}
