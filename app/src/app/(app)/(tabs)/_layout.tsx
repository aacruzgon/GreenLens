import { useRouter } from "expo-router";
import { NativeTabs } from "expo-router/unstable-native-tabs";

import { useCanScan } from "@/features/capture";
import { useTheme } from "@/ui";

// Native tabs (roadmap F0 item 8). The chosen tab is in the text colour, not green: controls are neutral,
// content is green (design 3.1). Tasks join in F4 and Chat in F6, so no tab is ever empty.
//
// The scan action (design open decision 2): iOS 26 draws a tab with the search role as a round button
// beside the tab bar, where the mockup puts the scan button. It is disabled, so a tap never selects it:
// the tab bar still reports the press, and the press opens the new-scan sheet. Hidden for viewers.
export default function TabsLayout() {
  const { colors } = useTheme();
  const router = useRouter();
  const canScan = useCanScan();
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
      <NativeTabs.Trigger
        name="scan"
        role="search"
        disabled
        hidden={!canScan}
        listeners={{ tabPress: () => router.push("/scan/new") }}
      >
        <NativeTabs.Trigger.Icon sf="camera.fill" />
        <NativeTabs.Trigger.Label>Scan</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
    </NativeTabs>
  );
}
