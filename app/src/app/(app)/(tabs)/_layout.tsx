import { NativeTabs } from "expo-router/unstable-native-tabs";

import { useTheme } from "@/ui";

// The system tab bar (HIG tab bars; .claude/skills/hig/expo.md). Tasks and Chat join as they're built.
export default function TabsLayout() {
  const t = useTheme();
  return (
    <NativeTabs tintColor={t.colors.accent}>
      <NativeTabs.Trigger name="index">
        <NativeTabs.Trigger.Label>Garden</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf={{ default: "leaf", selected: "leaf.fill" }} md="yard" />
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="settings">
        <NativeTabs.Trigger.Label>Settings</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf="gearshape" md="settings" />
      </NativeTabs.Trigger>
    </NativeTabs>
  );
}
